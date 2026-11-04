"""The interpretability engine.

Runs the perturbation suite against the audited model, asks the same question
several times, takes the prompt apart word by word, checks the model's own account
of its answer, and rolls everything up into a Report.

Progress goes out through `emit(event, data)` so a UI can show it live.
"""
from __future__ import annotations

import asyncio
import time
from collections.abc import Awaitable, Callable
from dataclasses import dataclass, field

from ..budget import Budget
from ..catalog import DEPTHS
from ..config import Settings
from ..sarvam import AuthError, ChatClient, OutOfCredits, SarvamError, TextClient
from . import faithfulness, importance, perturb, scoring
from .models import AuditRequest, Faithfulness, Report, TestResult
from .similarity import SCORES, Judge, lexical_similarity, lexical_verdict, settled_without_judge
from .targets import Answer, Target, TargetError

Emit = Callable[[str, dict], Awaitable[None]]
RUN_TIMEOUT_SECONDS = 480.0
CALL_TIMEOUT_SECONDS = 120.0
RESAMPLE_TEMPERATURE = 0.8


class AuditFailed(Exception):
    """Something went wrong that the person should hear about in plain words."""


@dataclass
class _Run:
    req: AuditRequest
    target: Target
    emit: Emit
    sem: asyncio.Semaphore
    notes: list[str]
    fatal: list[Exception] = field(default_factory=list)
    done: int = 0
    total: int = 0

    async def ask(self, text: str, *, temperature: float = 0.0, n: int = 1) -> list[Answer]:
        if self.fatal:
            raise self.fatal[0]
        async with self.sem:
            if self.fatal:
                raise self.fatal[0]
            try:
                return await asyncio.wait_for(self.target.ask(text, temperature=temperature, n=n), CALL_TIMEOUT_SECONDS)
            except (AuthError, OutOfCredits) as exc:
                self.fatal.append(exc)
                raise
            except (SarvamError, TargetError) as exc:
                return [Answer("", error=str(exc)) for _ in range(n)]
            except TimeoutError:
                return [Answer("", error="The model took too long to answer.") for _ in range(n)]

    async def tick(self) -> None:
        self.done += 1
        await self.emit("progress", {"done": self.done, "total": self.total})


class Engine:
    def __init__(self, settings: Settings, chat: ChatClient, text: TextClient, budget: Budget):
        self.settings = settings
        self.chat = chat
        self.text = text
        self.budget = budget
        self.judge = Judge(chat, settings.judge_model)

    async def run(self, req: AuditRequest, target: Target, emit: Emit) -> Report:
        run = _Run(req, target, emit, asyncio.Semaphore(self.settings.parallel_calls), list(req.plan.notes))
        started = time.monotonic()
        try:
            async with asyncio.timeout(RUN_TIMEOUT_SECONDS):
                report = await self._run(run)
        except TimeoutError:
            raise AuditFailed("This audit was taking too long, so I stopped it. Try a quicker depth or a shorter question.") from None
        report.seconds = round(time.monotonic() - started, 1)
        return report

    # --- the run ---------------------------------------------------------------

    async def _run(self, run: _Run) -> Report:
        plan = run.req.plan
        prompt, lang = plan.prompt, plan.prompt_language
        preset = DEPTHS[plan.depth]
        types = set(plan.audit_types)

        await run.emit("phase", {"name": "variants"})
        variants = await self._variants(run, prompt, lang, preset, types)
        units = importance.make_units(prompt, preset["units"]) if types & {"perturbation", "behavioral"} else []
        resamples = preset["resamples"] if "consistency" in types else 0
        await run.emit("plan", {
            "variants": [{"id": v.id, "category": v.category, "kind": v.kind, "label": v.label, "input": v.text} for v in variants],
            "units": [{"index": u.index, "text": u.text, "start": u.start, "end": u.end} for u in units],
            "resamples": resamples,
        })
        if not variants and not resamples:
            raise AuditFailed("There was nothing to test for the checks you picked. Try a longer question or another check.")

        await run.emit("phase", {"name": "baseline"})
        first = (await run.ask(prompt))[0]
        if first.error or not first.text.strip():
            raise AuditFailed(f"The model didn't answer your question ({first.error or 'it sent back nothing'}), so there's nothing to compare against.")
        baseline = first.text
        await run.emit("baseline", {"output": baseline})

        await run.emit("phase", {"name": "probe"})
        answers = await self._probe(run, prompt, variants, units, resamples)

        await run.emit("phase", {"name": "compare"})
        outputs = self._outputs(variants, units, resamples, answers, plan.reference_output)
        verdicts, judge_used = await self._compare(run, prompt, baseline, outputs)

        await run.emit("phase", {"name": "score"})
        tests = await self._tests(run, prompt, variants, resamples, answers, verdicts)
        ranked = importance.rank_units(units, self._unit_raws(units, baseline, outputs, verdicts))
        await run.emit("units", {"units": [u.model_dump() for u in ranked]})

        faith = self._faithfulness(answers, ranked)
        score, total, unexpected, skipped = scoring.consistency(tests)
        if total == 0:
            raise AuditFailed("None of the tests came back with an answer I could compare. Please try again in a moment.")

        reference_agrees = None
        if plan.reference_output and "reference" in verdicts:
            reference_agrees = verdicts["reference"][0] != "different"

        groups = scoring.group_scores(tests)
        flags = scoring.build_flags(
            tests, ranked, faith, baseline_output=baseline, prompt_language=lang, reference_agrees=reference_agrees,
        )
        if skipped:
            run.notes.append(f"{skipped} test{'s' if skipped != 1 else ''} got no answer and {'were' if skipped != 1 else 'was'} left out of the score.")
        if not judge_used and any(not t.judged for t in tests if t.status != "error"):
            run.notes.append("The comparison helper wasn't available, so answers were compared by wording alone. Treat the score as rougher than usual.")

        report = Report(
            prompt=prompt, prompt_language=lang, baseline_output=baseline, reference_output=plan.reference_output,
            target_label=run.target.label, depth=plan.depth, audit_types=plan.audit_types,
            consistency_score=score, grade=scoring.grade_for(score),
            total_perturbations=total, unexpected_changes=unexpected, skipped=skipped,
            tests=tests, groups=groups, units=ranked, faithfulness=faith, flags=flags,
            strengths=scoring.strengths(groups), next_steps=scoring.next_steps(flags),
            next_step_codes=scoring.next_step_codes(flags),
            not_tested=scoring.not_tested(plan.audit_types, plan.depth, groups),
            not_tested_codes=scoring.not_tested_codes(plan.audit_types, plan.depth, groups),
            notes=run.notes, judge_used=judge_used,
        )
        return report

    # --- steps -------------------------------------------------------------------

    async def _variants(self, run: _Run, prompt: str, lang: str, preset: dict, types: set[str]) -> list[perturb.Variant]:
        rng = perturb.seeded_rng(prompt)
        found: list[perturb.Variant] = []
        if "perturbation" in types:
            found += perturb.typo_variants(prompt, preset["typo"], rng)
            found += perturb.format_variants(prompt, lang, preset["format"])
            numerals = perturb.numeral_variant(prompt, lang)
            if numerals:
                found.append(numerals)
            found += perturb.distractor_variants(prompt, lang, preset["distractor"])
            found += perturb.polite_variants(prompt, lang, preset["polite"])
        if "behavioral" in types:
            found += perturb.control_variants(prompt, lang, preset["control"])

        async def attempt(what: str, coro):
            try:
                return await coro
            except (AuthError, OutOfCredits):
                raise
            except Exception:
                run.notes.append(f"I couldn't prepare {what} this time, so that part was skipped.")
                return None

        s = self.settings
        jobs: dict[str, Awaitable] = {}
        if "perturbation" in types:
            jobs["paraphrase"] = attempt("rewordings", perturb.paraphrase_variants(self.chat, s.plan_model, prompt, lang, preset["paraphrase"]))
            jobs["script"] = attempt("the English-letters version", perturb.roman_variant(self.text, prompt, lang))
            jobs["language"] = attempt("the other-language versions", perturb.language_variants(self.text, prompt, lang, preset["language"]))
        if "behavioral" in types:
            jobs["negation"] = attempt("the reversed-meaning question", perturb.opposite_variants(self.chat, s.plan_model, prompt, lang, preset["negation"]))
        if jobs:
            got = dict(zip(jobs, await asyncio.gather(*jobs.values())))
            found += got.get("paraphrase") or []
            if got.get("script"):
                found.append(got["script"])
            if got.get("language"):
                made, problems = got["language"]
                found += made
                run.notes += [f"I {p}." for p in problems]
            found += got.get("negation") or []
            if "paraphrase" in got and not got["paraphrase"] and preset["paraphrase"]:
                run.notes.append("No usable rewordings came back, so that group is missing.")

        unique, seen = [], {prompt.strip()}
        for v in found:
            if v.text.strip() and v.text.strip() not in seen:
                seen.add(v.text.strip())
                unique.append(v)
        return unique

    async def _probe(self, run: _Run, prompt, variants, units, resamples) -> dict:
        """Ask the audited model everything. Returns answers keyed by test id."""
        out: dict = {"variants": {}, "units": {}, "resamples": [], "probe": None}
        probing = bool(units) and run.target.can_explain
        run.total = len(variants) + len(units) + (1 if resamples else 0) + (1 if probing else 0)
        await run.emit("progress", {"done": 0, "total": run.total})

        async def one_variant(v: perturb.Variant) -> None:
            ans = (await run.ask(v.text))[0]
            out["variants"][v.id] = ans
            await run.emit("answer", {"id": v.id, "output": None if ans.error else ans.text, "error": ans.error})
            await run.tick()

        async def one_unit(u: importance.UnitSpec) -> None:
            out["units"][u.index] = (await run.ask(importance.without_unit(prompt, u)))[0]
            await run.tick()

        async def repeats() -> None:
            out["resamples"] = await run.ask(prompt, temperature=RESAMPLE_TEMPERATURE, n=resamples)
            for i, ans in enumerate(out["resamples"], 1):
                await run.emit("answer", {"id": f"resample-{i}", "output": None if ans.error else ans.text, "error": ans.error})
            await run.tick()

        async def self_report() -> None:
            out["probe"] = (await run.ask(prompt + faithfulness.PROBE_SUFFIX))[0]
            await run.tick()

        jobs = [one_variant(v) for v in variants] + [one_unit(u) for u in units]
        if resamples:
            jobs.append(repeats())
        if probing:
            jobs.append(self_report())
        try:
            async with asyncio.TaskGroup() as group:
                for job in jobs:
                    group.create_task(job)
        except ExceptionGroup as group_error:
            raise group_error.exceptions[0] from None
        return out

    @staticmethod
    def _outputs(variants, units, resamples, answers, reference) -> dict[str, str]:
        outputs: dict[str, str] = {}
        for v in variants:
            ans = answers["variants"].get(v.id)
            if ans and not ans.error and ans.text.strip():
                outputs[v.id] = ans.text
        for i, ans in enumerate(answers["resamples"], 1):
            if not ans.error and ans.text.strip():
                outputs[f"resample-{i}"] = ans.text
        for u in units:
            ans = answers["units"].get(u.index)
            if ans and not ans.error and ans.text.strip():
                outputs[f"unit-{u.index}"] = ans.text
        if reference:
            outputs["reference"] = reference
        return outputs

    async def _compare(self, run: _Run, prompt: str, baseline: str, outputs: dict[str, str]):
        """verdicts: id -> (verdict, reason, judged). Cheap checks first, the judge for the rest."""
        verdicts: dict[str, tuple[str, str, bool]] = {}
        undecided: dict[str, str] = {}
        for key, text in outputs.items():
            if settled_without_judge(baseline, text):
                verdicts[key] = ("same", "", False)
            else:
                undecided[key] = text
        judged: dict = {}
        if undecided:
            try:
                judged = await self.judge.compare(prompt, baseline, undecided)
            except (AuthError, OutOfCredits):
                raise
            except Exception:
                judged = {}
        for key, text in undecided.items():
            if key in judged:
                verdicts[key] = (judged[key].verdict, judged[key].why, True)
            else:
                verdicts[key] = (lexical_verdict(baseline, text)[0], "", False)
        return verdicts, bool(judged) or not undecided

    async def _tests(self, run: _Run, prompt, variants, resamples, answers, verdicts) -> list[TestResult]:
        tests: list[TestResult] = []

        def build(test_id, category, kind, label, text, ans: Answer | None, note="") -> TestResult:
            t = TestResult(id=test_id, category=category, kind=kind, label=label, input=text, note=note)
            if ans is not None and not ans.error and ans.text.strip():
                verdict, why, judged = verdicts[test_id]
                t.output, t.verdict, t.sim, t.reason, t.judged = ans.text, verdict, SCORES[verdict], why, judged
            elif ans is not None and ans.error:
                t.reason = ans.error
            return scoring.finish_test(t)

        for v in variants:
            tests.append(build(v.id, v.category, v.kind, v.label, v.text, answers["variants"].get(v.id), v.note))
        for i, ans in enumerate(answers["resamples"], 1):
            tests.append(build(f"resample-{i}", "resample", "resample", f"Asked again (try {i})", prompt, ans))
        for t in tests:
            await run.emit("test", t.model_dump())
        return tests

    @staticmethod
    def _unit_raws(units, baseline, outputs, verdicts) -> dict[int, float]:
        raws = {}
        for u in units:
            key = f"unit-{u.index}"
            if key in outputs:
                sim = SCORES[verdicts[key][0]]
                raws[u.index] = 0.8 * (1 - sim) + 0.2 * (1 - lexical_similarity(baseline, outputs[key]))
        return raws

    @staticmethod
    def _faithfulness(answers, ranked) -> Faithfulness:
        probe = answers.get("probe")
        if not ranked or probe is None or probe.error:
            return Faithfulness(status="skipped", note="Not run for this audit.")
        _, phrases = faithfulness.parse_key_line(probe.text)
        return faithfulness.evaluate(phrases, ranked)
