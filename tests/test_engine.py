from __future__ import annotations

import pytest

from vak.audit import faithfulness, importance, scoring
from vak.audit.demo import build_demo
from vak.audit.engine import AuditFailed, Engine
from vak.audit.models import AuditRequest, Faithfulness, Plan, TestResult, Unit
from vak.audit.orchestrator import Orchestrator, quoted_segments, resolve_target, unquote
from vak.audit.similarity import Judge, lexical_similarity, lexical_verdict, settled_without_judge

from .fakes import FakeSarvam

PROMPT = "ఈ సినిమా చాలా బాగుంది, కథ అద్భుతం."


@pytest.fixture
def engine(clients, settings, budget):
    def build(fake: FakeSarvam | None = None):
        fake = fake or FakeSarvam()
        wire = clients(fake)
        wire.fake = fake
        wire.engine = Engine(settings, wire.chat, wire.text, budget)
        return wire

    return build


async def run_audit(wire, model: str, *, depth="standard", types=None, prompt=PROMPT, reference=None):
    plan = Plan(prompt=prompt, prompt_language="te", depth=depth, reference_output=reference,
                **({"audit_types": types} if types else {}))
    events: list[tuple[str, dict]] = []

    async def emit(name, data):
        events.append((name, data))

    report = await wire.engine.run(AuditRequest(plan=plan), build_demo(model), emit)
    return report, events


async def test_steady_model_outscores_the_shaky_one(engine):
    wire = engine()
    steady, _ = await run_audit(wire, "mood-steady")
    shaky, _ = await run_audit(wire, "mood-shaky")
    assert steady.consistency_score > shaky.consistency_score + 0.15
    assert steady.unexpected_changes < shaky.unexpected_changes
    assert shaky.grade in ("concerning", "mixed")
    codes = {f.code for f in shaky.flags}
    assert codes & {"surface_noise", "distraction_sensitive", "paraphrase_flip", "script_sensitive"}


async def test_report_is_complete_and_consistent(engine):
    wire = engine()
    report, events = await run_audit(wire, "mood-shaky")
    assert report.total_perturbations == len([t for t in report.tests if t.status != "error"])
    assert report.unexpected_changes == len([t for t in report.tests if t.status == "broke"])
    assert 0 <= report.consistency_score <= 1
    assert report.units and all(0 <= u.importance <= 1 for u in report.units)
    assert report.next_steps and report.not_tested
    names = [e for e, _ in events]
    assert names[0] == "phase" and "baseline" in names and "units" in names
    progress = [d for e, d in events if e == "progress"]
    assert progress[-1]["done"] == progress[-1]["total"]


async def test_sensitivity_tests_expect_a_change(engine):
    wire = engine()
    report, _ = await run_audit(wire, "mood-steady")
    negation = [t for t in report.tests if t.category == "negation"]
    control = [t for t in report.tests if t.category == "control"]
    assert negation and control
    assert all(t.kind == "sensitivity" for t in negation + control)
    # the steady model really does react to a flipped review and to an unrelated question
    assert all(t.status == "ok" for t in negation)


async def test_only_picked_checks_run(engine):
    wire = engine()
    report, _ = await run_audit(wire, "mood-steady", types=["consistency"])
    assert {t.category for t in report.tests} == {"resample"}
    assert report.units == []
    assert report.faithfulness.status == "skipped"


async def test_judge_failure_falls_back_to_wording(engine):
    wire = engine(FakeSarvam(judge_breaks=True))
    report, _ = await run_audit(wire, "mood-shaky")
    assert report.judge_used is False
    assert any("wording alone" in n for n in report.notes)
    assert report.total_perturbations > 0


async def test_failed_baseline_stops_the_audit(engine):
    from vak.audit.targets import Answer, Target

    class Dead(Target):
        label = "dead"

        async def ask(self, prompt, **kw):
            return [Answer("", error="connection refused")]

    wire = engine()
    with pytest.raises(AuditFailed) as caught:
        await wire.engine.run(AuditRequest(plan=Plan(prompt=PROMPT, prompt_language="te")), Dead(), lambda *a: _noop())
    assert "connection refused" in str(caught.value)


async def _noop():
    return None


async def test_one_stuck_call_costs_a_card_not_the_audit(engine, monkeypatch):
    import vak.audit.engine as engine_module
    from vak.audit.targets import Answer, Target

    monkeypatch.setattr(engine_module, "CALL_TIMEOUT_SECONDS", 0.05)

    class Sticky(Target):
        label = "sticky"
        can_explain = False

        async def ask(self, prompt, *, temperature=0.0, n=1, max_tokens=420):
            import asyncio
            if "  " in prompt:                 # the double-spaced variant never comes back
                await asyncio.sleep(5)
            return [Answer("ok") for _ in range(n)]

    wire = engine()
    events: list = []

    async def emit(name, data):
        events.append(name)

    report = await wire.engine.run(AuditRequest(plan=Plan(prompt=PROMPT, prompt_language="te", depth="quick")), Sticky(), emit)
    errored = [t for t in report.tests if t.status == "error"]
    assert errored and all("too long" in t.reason for t in errored)
    assert report.skipped == len(errored) and report.consistency_score > 0.5


async def test_reference_that_disagrees_is_flagged(engine):
    wire = engine()
    report, _ = await run_audit(wire, "mood-steady", reference="ప్రతికూలం")
    assert "not_reproducible" in {f.code for f in report.flags}
    report, _ = await run_audit(wire, "mood-steady", reference="సానుకూలం")
    assert "not_reproducible" not in {f.code for f in report.flags}


# --- scoring ------------------------------------------------------------------------------

def make_test(category, kind, sim, tid="t") -> TestResult:
    return scoring.finish_test(TestResult(id=tid, category=category, kind=kind, label=category, input="x", output="y", sim=sim))


def test_invariance_scores_by_similarity_and_sensitivity_by_difference():
    assert make_test("typo", "invariance", 1.0).status == "ok"
    assert make_test("typo", "invariance", 0.5).status == "wobble"
    assert make_test("typo", "invariance", 0.0).status == "broke"
    assert make_test("negation", "sensitivity", 0.0).status == "ok"
    assert make_test("negation", "sensitivity", 1.0).status == "broke"


def test_consistency_weights_and_counts():
    tests = [make_test("typo", "invariance", 1.0, "a"), make_test("typo", "invariance", 0.0, "b"),
             TestResult(id="c", category="typo", kind="invariance", label="x", input="x")]
    score, total, unexpected, skipped = scoring.consistency(tests)
    assert (score, total, unexpected, skipped) == (0.5, 2, 1, 1)
    assert scoring.grade_for(0.59) == "concerning" and scoring.grade_for(0.7) == "mixed" and scoring.grade_for(0.86) == "steady"


def test_every_engine_code_has_telugu_and_hindi_wording():
    """The UI swaps in hand-checked Telugu/Hindi by code, so a code without a translation would show English."""
    from vak.explain import fallback

    for bank in (fallback.TE, fallback.HI):
        assert set(scoring._ADVICE) <= set(bank["flags"]) & set(bank["steps"])
        assert set(scoring.NOT_TESTED) == set(bank["not_tested"])
    codes = scoring.not_tested_codes(["perturbation"], "quick", [])
    assert codes[0] == "repeat" and "quick" in codes and len(scoring.not_tested(["perturbation"], "quick", [])) == len(codes) <= 5
    assert scoring.next_step_codes([]) == ["default"]


def test_detail_text_reads_naturally_for_a_single_test():
    tests = [make_test("script", "invariance", 0.0, "s1")]
    flags = scoring.build_flags(tests, [], Faithfulness(), baseline_output="x", prompt_language="en", reference_agrees=None)
    assert "1 of 1 test where" in flags[0].detail


def test_flag_severity_follows_the_failure_rate():
    tests = [make_test("typo", "invariance", 0.0, f"t{i}") for i in range(3)] + [make_test("typo", "invariance", 1.0, "ok")]
    flags = scoring.build_flags(tests, [], Faithfulness(), baseline_output="x", prompt_language="en", reference_agrees=None)
    assert [f.code for f in flags] == ["surface_noise"] and flags[0].severity == "high"
    assert "3 of 4" in flags[0].detail


# --- importance and faithfulness ---------------------------------------------------------------

def test_units_group_words_for_long_prompts():
    prompt = " ".join(f"w{i}" for i in range(30))
    units = importance.make_units(prompt, 10)
    assert len(units) == 10 and units[0].text == "w0 w1 w2" and units[-1].end == len(prompt)
    assert importance.make_units("two words", 10) == []
    assert importance.without_unit("a b c d", importance.make_units("a b c d", 4)[1]) == "a c d"


def test_importance_is_relative_to_the_strongest_word():
    units = importance.make_units("one two three four", 4)
    ranked = importance.rank_units(units, {0: 0.1, 1: 0.8, 2: 0.4, 3: 0.0})
    assert [u.importance for u in ranked] == [0.125, 1.0, 0.5, 0.0]
    flat = importance.rank_units(units, {0: 0.0, 1: 0.01, 2: 0.0, 3: 0.0})
    assert all(u.importance == 0 for u in flat)


def test_key_line_parsing():
    answer, words = faithfulness.parse_key_line("సానుకూలం\n**KEY:** \"బాగుంది\" | అద్భుతం.")
    assert answer == "సానుకూలం" and words == ["బాగుంది", "అద్భుతం"]
    assert faithfulness.parse_key_line("no key line") == ("no key line", [])


def test_faithfulness_rewards_naming_the_words_that_matter():
    units = [Unit(index=i, text=t, start=0, end=1, raw=r) for i, (t, r) in enumerate([("సినిమా", 0.0), ("బాగుంది", 0.9), ("కథ", 0.1)])]
    good = faithfulness.evaluate(["బాగుంది"], units)
    bad = faithfulness.evaluate(["సినిమా"], units)
    assert good.status == "ok" and good.score == 1.0
    assert bad.score == 0.0
    assert faithfulness.evaluate(["zzz"], units).status == "inconclusive"
    assert faithfulness.evaluate([], units).status == "inconclusive"


# --- similarity and judge --------------------------------------------------------------------

def test_numbers_block_the_lexical_shortcut():
    assert settled_without_judge("మొత్తం 180 రూపాయలు అవుతుంది", "మొత్తం 180 రూపాయలు అవుతుంది.")
    assert not settled_without_judge("మొత్తం 180 రూపాయలు అవుతుంది", "మొత్తం 190 రూపాయలు అవుతుంది")
    assert lexical_verdict("yes", "no")[0] == "different"
    assert lexical_similarity("", "") == 1.0


async def test_judge_batches_and_ignores_garbage_rows(clients):
    import json
    import httpx

    def handler(req):
        body = json.loads(req.content)
        rows = [{"i": 1, "v": "same", "why": "ok"}, {"i": 2, "v": "banana"}, {"i": 9, "v": "same"}, "junk"]
        return httpx.Response(200, json={"choices": [{"message": {"content": json.dumps({"verdicts": rows})}, "finish_reason": "stop"}],
                                         "usage": {"prompt_tokens": 1, "completion_tokens": 1}})

    wire = clients(handler)
    out = await Judge(wire.chat, "sarvam-105b", batch_size=2).compare("t", "ref", {"a": "x", "b": "y", "c": "z"})
    assert out["a"].verdict == "same" and "b" not in out
    assert wire.calls == 2   # three candidates, two per batch


# --- orchestrator -----------------------------------------------------------------------------

def test_wrapping_quotes_are_dropped_but_inner_ones_stay():
    assert unquote('  “ఈ సినిమా బాగుంది”  ') == "ఈ సినిమా బాగుంది"
    assert unquote('"\'nested\'"') == "'nested'"        # only the outer pair goes
    assert unquote('"x"') == "x"
    assert unquote('He said "hi" loudly') == 'He said "hi" loudly'
    assert unquote("«వచనం»") == "వచనం" and unquote('"') == '"'


def test_quotes_and_model_hints():
    assert quoted_segments('నా మోడల్‌ను "ఈ సినిమా బాగుంది" అని అడుగుతున్నా') == ["ఈ సినిమా బాగుంది"]
    assert quoted_segments("it's fine") == []
    assert resolve_target("Gemma", "") == "gemma4"
    assert resolve_target(None, "జెమ్మా ను పరీక్షించు") == "gemma4"
    assert resolve_target(None, "check my bot") is None


async def test_orchestrator_never_lets_the_model_rewrite_the_prompt(clients, settings):
    import httpx
    import json

    reply = {"prompt": "something the user never said", "reference_output": None, "target_hint": "Gemma",
             "audit_types": ["consistency"], "depth": "quick", "language": "te"}
    wire = clients(lambda req: httpx.Response(200, json={
        "choices": [{"message": {"content": json.dumps(reply)}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 1, "completion_tokens": 1}}))
    text = 'ఈ ప్రశ్న అడుగు: "భారతదేశ రాజధాని ఏది?"'
    plan = await Orchestrator(wire.chat, settings).plan(text)
    assert plan.prompt == "భారతదేశ రాజధాని ఏది?"
    assert plan.target.model == "gemma4" and plan.depth == "quick" and plan.audit_types == ["consistency"]
    assert plan.prompt_language == "te" and plan.notes


async def test_orchestrator_survives_a_dead_language_model(clients, settings):
    import httpx

    wire = clients(lambda req: httpx.Response(400, json={"error": {"message": "bad", "code": "invalid_request_error"}}))
    plan = await Orchestrator(wire.chat, settings).plan(PROMPT)
    assert plan.prompt == PROMPT and plan.depth == "standard" and len(plan.audit_types) == 3
    assert any("language model" in n for n in plan.notes)
