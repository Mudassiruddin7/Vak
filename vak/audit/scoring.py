"""From raw test outcomes to a score, flags and advice.

The consistency score is a weighted average of how well the model behaved in each
test (1 = exactly as it should). Meaning-preserving edits should leave the answer
alone, so a score is the similarity of the two answers. Meaning-changing edits
should move it, so there the score is 1 minus the similarity.
"""
from __future__ import annotations

from collections import defaultdict

from .models import Faithfulness, Flag, Group, TestResult, Unit
from .text_utils import script_share

CATEGORY_WEIGHT = {
    "resample": 1.0, "typo": 1.0, "format": 1.0, "numerals": 1.0, "script": 1.0,
    "paraphrase": 1.0, "language": 0.7, "distractor": 1.0, "polite": 0.7,
    "negation": 1.0, "control": 0.5,
}

GROUPS = [
    ("repeat", "Asking again", ("resample",)),
    ("noise", "Typos and formatting", ("typo", "format", "numerals", "script")),
    ("wording", "Different wording", ("paraphrase",)),
    ("language", "Other languages", ("language",)),
    ("distraction", "Distractions and manners", ("distractor", "polite")),
    ("meaning", "Meaning changes", ("negation", "control")),
]

OK_AT = 0.8
WOBBLE_AT = 0.4
STEADY_ABOVE = 0.85
CONCERN_BELOW = 0.6


def finish_test(t: TestResult) -> TestResult:
    """Fill in score and status from the similarity."""
    if t.output is None or t.sim is None:
        t.score, t.status = None, "error"
        return t
    t.score = round(1.0 - t.sim if t.kind == "sensitivity" else t.sim, 3)
    t.status = "ok" if t.score >= OK_AT else "wobble" if t.score >= WOBBLE_AT else "broke"
    return t


def grade_for(score: float) -> str:
    if score < CONCERN_BELOW:
        return "concerning"
    return "steady" if score > STEADY_ABOVE else "mixed"


def consistency(tests: list[TestResult]) -> tuple[float, int, int, int]:
    """(score, tests counted, unexpected changes, tests that errored out)."""
    valid = [t for t in tests if t.status != "error" and t.score is not None]
    skipped = len(tests) - len(valid)
    if not valid:
        return 0.0, 0, 0, skipped
    weights = [CATEGORY_WEIGHT.get(t.category, 1.0) for t in valid]
    score = sum(w * t.score for w, t in zip(weights, valid)) / sum(weights)
    return round(score, 2), len(valid), sum(t.status == "broke" for t in valid), skipped


def group_scores(tests: list[TestResult]) -> list[Group]:
    out = []
    for key, label, cats in GROUPS:
        members = [t for t in tests if t.category in cats and t.score is not None]
        if not members:
            continue
        weights = [CATEGORY_WEIGHT.get(t.category, 1.0) for t in members]
        score = sum(w * t.score for w, t in zip(weights, members)) / sum(weights)
        out.append(Group(key=key, label=label, score=round(score, 2), n=len(members)))
    return out


# --- flags -------------------------------------------------------------------------------

def _members(tests: list[TestResult], *cats: str) -> list[TestResult]:
    return [t for t in tests if t.category in cats and t.status != "error"]


def _severity(bad: int, n: int) -> str:
    rate = bad / n
    return "high" if rate >= 0.67 else "medium" if rate >= 0.34 else "low"


def build_flags(
    tests: list[TestResult],
    units: list[Unit],
    faith: Faithfulness,
    *,
    baseline_output: str,
    prompt_language: str,
    reference_agrees: bool | None,
) -> list[Flag]:
    flags: list[Flag] = []

    def invariance_flag(code: str, title: str, cats: tuple[str, ...], what: str) -> None:
        pool = _members(tests, *cats)
        broke = [t for t in pool if t.status == "broke"]
        if broke:
            flags.append(Flag(
                code=code, severity=_severity(len(broke), len(pool)), title=title,
                detail=f"The answer changed in {len(broke)} of {len(pool)} tests where only {what} was different.",
                evidence=[t.id for t in broke],
            ))

    invariance_flag("surface_noise", "Small typing differences change the answer", ("typo", "format", "numerals"),
                    "spelling, spacing or digits")
    invariance_flag("script_sensitive", "Answer changes when typed in English letters", ("script",),
                    "the letters used to type it (English letters instead of the native script)")
    invariance_flag("paraphrase_flip", "Rewording the question changes the answer", ("paraphrase",), "the wording")
    invariance_flag("language_flip", "Different languages give different answers", ("language",), "the language")
    invariance_flag("distraction_sensitive", "Side remarks throw it off", ("distractor", "polite"),
                    "an extra unrelated or polite sentence was added")

    repeats = _members(tests, "resample")
    if repeats:
        mean = sum(t.score for t in repeats) / len(repeats)
        if mean < 0.75:
            flags.append(Flag(
                code="unstable_sampling", severity="high" if mean < 0.5 else "medium",
                title="Asking the same thing again gives a different answer",
                detail=f"Asked the identical question {len(repeats)} more times, the answers agreed with the first only about {round(mean * 100)}% of the time.",
                evidence=[t.id for t in repeats if t.status != "ok"],
            ))

    flipped = [t for t in _members(tests, "negation") if t.status == "broke"]
    if flipped:
        pool = _members(tests, "negation")
        flags.append(Flag(
            code="negation_blind", severity="high" if len(flipped) == len(pool) else "medium",
            title="Doesn't notice when the meaning is reversed",
            detail=f"When the question was changed to mean the opposite, the answer stayed the same in {len(flipped)} of {len(pool)} tests.",
            evidence=[t.id for t in flipped],
        ))

    same_for_unrelated = [t for t in _members(tests, "control") if t.status == "broke"]
    if same_for_unrelated:
        flags.append(Flag(
            code="ignores_input", severity="high",
            title="Gives the same answer to an unrelated question",
            detail="Given a completely different question, it still produced nearly the same answer, so the answer may not depend on what you ask.",
            evidence=[t.id for t in same_for_unrelated],
        ))

    raws = sorted((u.raw for u in units), reverse=True)
    if len(raws) >= 4 and raws[0] >= 0.5 and sum(raws) > 0 and raws[0] / sum(raws) >= 0.6:
        top = max(units, key=lambda u: u.raw)
        flags.append(Flag(
            code="single_word_dependence", severity="medium",
            title="The answer hangs on a single word",
            detail=f"Removing just “{top.text}” changed the answer, while taking out any other word hardly mattered.",
            evidence=[],
        ))

    if faith.status == "ok" and faith.score is not None and faith.score < 0.4:
        flags.append(Flag(
            code="explanation_mismatch", severity="medium" if faith.score < 0.2 else "low",
            title="Its own reasons don't match what really moves it",
            detail="The words the model said it relied on made little difference when removed, while other words mattered more. This check is a rough hint, based on a single try.",
            evidence=[],
        ))

    if prompt_language in ("te", "hi"):
        outputs = [baseline_output] + [t.output for t in tests if t.kind != "sensitivity" and t.category != "language" and t.output]
        wrong = sum(1 for o in outputs if o and script_share(o, prompt_language) < 0.35)
        if wrong / len(outputs) >= 0.5:
            flags.append(Flag(
                code="language_drift", severity="low",
                title="Replies in a different language than the question",
                detail="Most answers came back in another language or script than the one the question was written in.",
                evidence=[],
            ))

    if reference_agrees is False:
        flags.append(Flag(
            code="not_reproducible", severity="medium",
            title="Today's answer doesn't match the one you saved",
            detail="The answer you gave as the reference is not what the model says now, even with the same question.",
            evidence=[],
        ))

    order = {"high": 0, "medium": 1, "low": 2}
    return sorted(flags, key=lambda f: order[f.severity])


# --- advice and honesty ------------------------------------------------------------------

_ADVICE = {
    "surface_noise": "Clean the text before it reaches the model: trim extra spaces, and normalise spelling, digits and punctuation.",
    "script_sensitive": "Decide how phone-typed English-letter input should be handled: convert it to the native script first, or add such cases to your tests.",
    "paraphrase_flip": "Keep several rewordings of every important question in your test set, and tighten the instructions so the answer doesn't depend on exact wording.",
    "language_flip": "Test each key question in every language your users speak, not just one.",
    "distraction_sensitive": "Say clearly in the instructions which part of the message is the task, and tell the model to ignore side remarks.",
    "unstable_sampling": "Lower the randomness setting, or ask three times and keep the answer most runs agree on.",
    "negation_blind": "Add opposite-meaning versions of your questions to your tests, since this model sometimes answers as if nothing changed.",
    "ignores_input": "Check that your question really reaches the model, because its answer barely reacts to what you write.",
    "single_word_dependence": "Make the instructions lean less on one keyword, and test what happens when that word is missing or misspelt.",
    "explanation_mismatch": "Don't treat the model's own reasons as proof. Judge it by tests like these.",
    "language_drift": "State the language of the reply in the instructions.",
    "not_reproducible": "Re-check the answer you saved, because the model doesn't give it today.",
}
_DEFAULT_ADVICE = "Keep these tests and run them again whenever you change the instructions or the model version."


def next_steps(flags: list[Flag]) -> list[str]:
    steps = [_ADVICE[f.code] for f in flags if f.code in _ADVICE]
    return steps[:4] or [_DEFAULT_ADVICE]


def strengths(groups: list[Group]) -> list[str]:
    out = []
    for g in groups:
        if g.score >= 0.9 and g.n >= 1:
            out.append(f"{g.label}: held steady in {g.n} test{'s' if g.n != 1 else ''}")
    return out[:4]


def not_tested(audit_types: list[str], depth: str, groups: list[Group]) -> list[str]:
    done = {g.key for g in groups}
    items = []
    if "language" not in done:
        items.append("Other languages: the question was not tried in a second language.")
    items += [
        "Very long inputs, and conversations with many turns.",
        "Inputs written to trick it on purpose, such as hidden instructions inside the question.",
        "Fairness: whether names, gender or community change the answer.",
        "How it behaves a month from now, after the model gets updated.",
    ]
    if depth == "quick":
        items.insert(0, "Most rewordings: this was a quick run with only a few variations.")
    if "consistency" not in audit_types:
        items.insert(0, "Asking the identical question several times.")
    return items[:5]
