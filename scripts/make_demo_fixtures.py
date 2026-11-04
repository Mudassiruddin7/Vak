"""Regenerate the two bundled example audits in vak/web/demo/ by running them for real.

    .venv/bin/python scripts/make_demo_fixtures.py

Both examples audit Sarvam 105B with your key (a couple of rupees in total). The "steady"
one is a plain price sum the model handles well; the "shaky" one is a small reasoning
puzzle it answers differently depending on wording and language. Only finished,
key-free records are written out.
"""
from __future__ import annotations

import asyncio
import json
import os
import tempfile
from pathlib import Path

from vak.audit.models import AuditRequest, Plan
from vak.config import Settings
from vak.service import Services

OUT = Path(__file__).resolve().parent.parent / "vak" / "web" / "demo"

EXAMPLES = {
    "steady": Plan(prompt="ఒక కిలో బియ్యం ధర 60 రూపాయలు అయితే 3 కిలోల ధర ఎంత?", prompt_language="te", depth="standard"),
    "shaky": Plan(prompt="ఒక గదిలో 3 కోళ్లు ఉన్నాయి. 2 కోళ్లు బయటకు వెళ్లిపోయాయి. గదిలో ఇంకా ఎన్ని కాళ్లు ఉన్నాయి?",
                  prompt_language="te", depth="standard"),
}


async def main() -> None:
    os.environ["VAK_DATA_DIR"] = tempfile.mkdtemp(prefix="vak-fixtures-")   # keep the real ledger out of it
    svc = Services(Settings.from_env())
    OUT.mkdir(parents=True, exist_ok=True)
    try:
        for name, plan in EXAMPLES.items():
            audit_id = await svc.start_audit(AuditRequest(plan=plan, language="te"))
            await svc.jobs.get(audit_id).task
            record = svc.store.audit_get(audit_id)
            if record["status"] != "done":
                raise SystemExit(f"{name}: audit ended as {record['status']}: {record.get('error')}")
            await svc.explanation_for(audit_id, "hi")
            record = svc.store.audit_get(audit_id)
            for ex in (record.get("explanations") or {}).values():
                ex.pop("audio", None)
            keep = {k: record[k] for k in ("id", "created", "status", "language", "prompt", "target", "depth", "score", "request", "report", "explanations", "error")}
            (OUT / f"{name}.json").write_text(json.dumps(keep, ensure_ascii=False, indent=1), encoding="utf-8")
            r = record["report"]
            print(f"{name}: score {r['consistency_score']} ({r['grade']}), {r['total_perturbations']} tests, "
                  f"{r['unexpected_changes']} unexpected, cost ₹{r['cost_inr']:.2f}")
    finally:
        await svc.aclose()


if __name__ == "__main__":
    asyncio.run(main())
