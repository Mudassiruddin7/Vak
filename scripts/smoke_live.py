"""Run one real audit against the live Sarvam API and print what happened.

It spends real (small) money: a quick audit is well under a rupee, a deep one a few.

    .venv/bin/python scripts/smoke_live.py --say 'ఈ ప్రశ్న పరీక్షించు: "భారతదేశ రాజధాని ఏది?"'
    .venv/bin/python scripts/smoke_live.py --prompt "ఒక కిలో బియ్యం ధర 60 రూపాయలు అయితే 3 కిలోల ధర ఎంత?" --depth standard --voice
"""
from __future__ import annotations

import argparse
import asyncio
import json
import time

from vak.audit.models import AuditRequest, Plan
from vak.config import Settings
from vak.explain.rules import check, word_count
from vak.service import Services

DEFAULT_PROMPT = "ఒక కిలో బియ్యం ధర 60 రూపాయలు అయితే 3 కిలోల ధర ఎంత?"


def frames(raw: str):
    event, data = None, None
    for line in raw.splitlines():
        if line.startswith("event: "):
            event = line[7:]
        elif line.startswith("data: "):
            data = json.loads(line[6:])
    return event, data


async def main(args: argparse.Namespace) -> None:
    settings = Settings.from_env()
    if not settings.has_key:
        raise SystemExit("No SARVAM_API_KEY found in .env")
    svc = Services(settings)
    started = time.monotonic()
    try:
        if args.say:
            made = await svc.make_plan(args.say, {"depth": args.depth})
            plan = Plan(**made["plan"])
            print(f"plan: prompt={plan.prompt!r} target={plan.target.model} types={plan.audit_types} depth={plan.depth}")
            for note in plan.notes:
                print("  note:", note)
        else:
            plan = Plan(prompt=args.prompt, prompt_language=args.prompt_lang, depth=args.depth)
        est = svc.estimate(plan, voice=args.voice)[plan.depth]
        print(f"estimate: up to ₹{est['total_inr']:.2f} (about {est['calls']} calls)")

        audit_id = await svc.start_audit(AuditRequest(plan=plan, language=args.lang, voice=args.voice))
        job = svc.jobs.get(audit_id)
        seen = 0
        async for frame in job.stream():
            event, data = frames(frame)
            if event in ("phase", "stage"):
                print(f"[{time.monotonic() - started:5.1f}s] {event}: {data}")
            elif event == "progress" and data["done"] in (data["total"], data["total"] // 2):
                print(f"[{time.monotonic() - started:5.1f}s] progress {data['done']}/{data['total']}")
            elif event in ("error", "notice", "cancelled"):
                print(f"[{time.monotonic() - started:5.1f}s] {event}: {data}")
            seen += 1

        rec = svc.store.audit_get(audit_id)
        if rec["status"] != "done":
            raise SystemExit(f"audit ended as {rec['status']}: {rec.get('error')}")
        report, ex = rec["report"], rec["explanations"][args.lang]

        print(f"\nbaseline answer: {report['baseline_output'][:160]!r}")
        print(f"score {report['consistency_score']} ({report['grade']}) | {report['total_perturbations']} tests, "
              f"{report['unexpected_changes']} unexpected, {report['skipped']} skipped | judge used: {report['judge_used']}")
        for t in report["tests"]:
            print(f"  {t['status']:6} {t['id']:14} {t['verdict'] or '-':9} {'judge' if t['judged'] else 'lex  '} | {t['input'][:44]!r} -> {(t['output'] or t['reason'])[:44]!r}")
        print("words:", [(u["text"], u["importance"]) for u in report["units"]])
        print("faithfulness:", report["faithfulness"]["status"], report["faithfulness"]["score"], report["faithfulness"]["claimed"])
        print("flags:", [(f["code"], f["severity"]) for f in report["flags"]])
        for note in report["notes"]:
            print("note:", note)

        print(f"\nexplanation ({ex['source']}, {ex['attempts']} attempt(s), {ex['model']}):")
        for s in ex["sentences"]:
            print(f"  [{word_count(s):2}w] {s}")
        found = check(ex["text"], args.lang)
        print(f"rule check: ok={found.ok} banned={found.banned} long={len(found.long_sentences)} latin_heavy={found.latin_heavy}")
        if args.voice and ex.get("audio"):
            print("voice:", ex["audio"])
        print(f"\ncost of this audit: ₹{report['cost_inr']:.3f} | wall time {time.monotonic() - started:.1f}s")
        print("budget:", {k: v["spent"] for k, v in svc.budget.summary()["pots"].items()})
    finally:
        await svc.aclose()


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--say", help="a natural-language request, as you'd speak it; goes through the orchestrator")
    ap.add_argument("--prompt", default=DEFAULT_PROMPT, help="the prompt to audit (ignored with --say)")
    ap.add_argument("--prompt-lang", default="te", choices=["te", "hi", "en"])
    ap.add_argument("--depth", default="quick", choices=["quick", "standard", "deep"])
    ap.add_argument("--lang", default="te", choices=["te", "hi"], help="explanation language")
    ap.add_argument("--voice", action="store_true", help="also read the explanation aloud")
    asyncio.run(main(ap.parse_args()))
