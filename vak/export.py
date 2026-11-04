"""A shareable Markdown write-up of one audit."""
from __future__ import annotations

import time

from .catalog import LANGS

_STATUS = {"ok": "held", "wobble": "wobbled", "broke": "broke", "error": "no answer"}


def _cell(text: str | None, limit: int = 70) -> str:
    text = (text or "").replace("|", "/").replace("\n", " ").strip()
    return text if len(text) <= limit else text[: limit - 1] + "…"


def report_markdown(record: dict) -> str:
    report = record["report"]
    out: list[str] = []
    when = time.strftime("%d %b %Y, %H:%M", time.localtime(record["created"]))
    out += [f"# Audit of “{_cell(report['prompt'], 90)}”", "", f"{report['target_label']} · {report['depth']} depth · {when}", ""]
    out += [f"**Consistency score: {round(report['consistency_score'] * 100)} / 100** ({report['grade']})  ",
            f"{report['total_perturbations']} tests, {report['unexpected_changes']} changed the answer when they shouldn't have.", ""]

    for lang, ex in (record.get("explanations") or {}).items():
        out += [f"## In {LANGS[lang].english}", "", ex["text"], ""]

    if report["flags"]:
        out += ["## What stood out", ""]
        out += [f"- **{f['title']}** ({f['severity']}). {f['detail']}" for f in report["flags"]]
        out.append("")
    if report["strengths"]:
        out += ["## What held up", ""] + [f"- {s}" for s in report["strengths"]] + [""]
    if report["next_steps"]:
        out += ["## What to do next", ""] + [f"- {s}" for s in report["next_steps"]] + [""]

    out += ["## Every test", "", "| Test | Input | Answer | Result |", "| --- | --- | --- | --- |"]
    for t in report["tests"]:
        out.append(f"| {t['label']} | {_cell(t['input'])} | {_cell(t['output'])} | {_STATUS.get(t['status'], t['status'])} |")
    out.append("")

    units = report["units"]
    if units:
        ranked = sorted(units, key=lambda u: u["importance"], reverse=True)[:5]
        out += ["## Words the answer leaned on", ""]
        out += [f"- {u['text']}: {round(u['importance'] * 100)}%" for u in ranked if u["importance"] > 0] or ["- No single word decided the answer."]
        out.append("")

    out += ["## Not tested", ""] + [f"- {s}" for s in report["not_tested"]] + [""]
    if report.get("notes"):
        out += ["## Notes", ""] + [f"- {n}" for n in report["notes"]] + [""]
    out.append(f"Baseline answer: {_cell(report['baseline_output'], 200)}  ")
    out.append(f"Cost of this audit: about ₹{report.get('cost_inr', 0):.2f}.")
    return "\n".join(out) + "\n"
