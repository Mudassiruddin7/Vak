"""Fills the system prompt for the explanation step."""
from __future__ import annotations

from pathlib import Path

from ..audit.models import Report
from ..audit.text_utils import trim_punct
from ..util import clip, digest
from . import fallback

TEMPLATE_DIR = Path(__file__).parent / "templates"

USER_MESSAGE = {
    "te": "Explain these audit results to me now, in Telugu, following every rule above.",
    "hi": "Explain these audit results to me now, in Hindi, following every rule above.",
}

# Everyday Indian scenes the explainer may borrow from. One is picked per audit so reports don't all sound alike.
ANALOGIES = [
    "A chai stall that makes the same cup of chai however you order it.",
    "An auto driver who reaches the same address even when you describe it in different words.",
    "A railway platform announcement that means the same thing even with noise in the background.",
    "A shopkeeper who quotes the same price whether you ask in Telugu, Hindi or English.",
    "A tailor who stitches the same shirt from the same measurements every time.",
    "A cook whose sambar tastes the same on a rainy day and on a sunny day.",
]


def _template(lang: str) -> str:
    return (TEMPLATE_DIR / f"system_{lang}.txt").read_text(encoding="utf-8")


def _faithfulness_line(report: Report) -> str:
    f = report.faithfulness
    if f.status == "ok" and f.score is not None:
        if f.score >= 0.7:
            return "Yes. The words it named are the ones that really move its answer (rough hint)."
        if f.score >= 0.4:
            return "Partly. Some of the words it named matter, others do not (rough hint)."
        return "Not really. The words it named made little difference, while other words mattered more (rough hint, based on one try)."
    if f.status == "inconclusive":
        return "Could not be checked."
    return "Not checked for this model."


def _top_word(report: Report) -> str:
    if not report.units:
        return "not measured"
    best = max(report.units, key=lambda u: u.raw)
    if best.raw < 0.4:
        return "none in particular; removing any single word did not change the answer"
    return f"“{trim_punct(best.text)}”"


def _in_language(report: Report, lang: str) -> tuple[str, str]:
    """This audit's findings and next steps, already phrased in plain Telugu or Hindi."""
    bank = fallback.TE if lang == "te" else fallback.HI
    codes = [f.code for f in report.flags][:3]
    findings = " ".join(bank["flags"][c] for c in codes if c in bank["flags"]) or bank["no_flags"]
    steps = " ".join(bank["steps"][c] for c in codes if c in bank["steps"]) or bank["steps"]["default"]
    return findings, steps


def _not_tested_in(lang: str) -> str:
    bank = fallback.TE if lang == "te" else fallback.HI
    return " ".join(bank["steady"][1:])


def build_system(report: Report, lang: str, seed: str) -> str:
    findings_local, steps_local = _in_language(report, lang)
    flags = "; ".join(f"{f.title} ({f.severity}): {f.detail}" for f in report.flags[:4]) or "none found in the tests that were run"
    analogy = ANALOGIES[int(digest(seed)[:8], 16) % len(ANALOGIES)]
    values = {
        "consistency_score": f"{report.consistency_score:.2f}",
        "flagged_behaviors": flags,
        "total_perturbations": report.total_perturbations,
        "unexpected_changes": report.unexpected_changes,
        "what_was_tested": f"the question “{clip(report.prompt, 140)}” put to {report.target_label}, "
                           f"with {report.total_perturbations} variations at {report.depth} depth",
        "score_100": round(report.consistency_score * 100),
        "strengths": "; ".join(report.strengths) or "nothing stood out yet",
        "top_word": _top_word(report),
        "faithfulness": _faithfulness_line(report),
        "not_tested": "; ".join(report.not_tested[:4]),
        "next_steps": " | ".join(report.next_steps[:4]),
        "analogy": analogy,
        "findings_local": findings_local,
        "not_tested_local": _not_tested_in(lang),
        "steps_local": steps_local,
    }
    return _template(lang).format(**values)


def build_user(lang: str) -> str:
    return USER_MESSAGE[lang]
