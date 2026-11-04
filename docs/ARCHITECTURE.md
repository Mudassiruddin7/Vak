# How Vak is put together

```
web/                 plain ES modules and CSS, no build step; embedded into the server binary
  i18n/              machine-translated interface strings for 20 languages (loaded on demand)
crates/
  vak-core/          pure logic, no I/O: text and script handling, the 23-language registry, writing rules and
                     their checker, perturbations, scoring and flags, effort and difficulty, the live trust score
  vak-app/           use cases behind traits: audit engine, judge, explainer, planner, live turns
  vak-adapters/      Sarvam clients (rate limit, retries, cache), custom endpoint with SSRF guard, Supabase
  vak-server/        axum: streaming audits (SSE), translate, live turns, voice, quotas, security headers
supabase/migrations  two tables and their row-level-security policies
scripts/             uv scripts that generate language data (wording banks, interface strings)
```

Dependencies point inward: the server knows the adapters, the adapters know the use cases, the core knows
nothing. Every use case is tested against scripted fakes of the network.

## Why Rust

An audit mostly waits on the network, so Rust does not make one call faster. It gives a small static binary that
starts in milliseconds, a few MB of memory per instance, cheap concurrency, and types that keep the report shape,
flag codes and wording banks in step (a test fails if any language is missing a flag text).

## Fewer calls

Tests run in two waves: one of each kind first, and a kind only gets more when its first came back less than clean.
One helper call writes rewordings and meaning-flips together; the judge answers in one letter per answer; obvious
matches never reach it; pasted text skips the planner; deterministic calls are cached in memory for an hour.

## Languages

Sarvam covers 22 scheduled languages plus English for speech in and translation, and 11 for voice. Written
explanations and the interface exist for all 23. Telugu, Hindi and English wording is hand-written; the others
are generated with Sarvam-Translate by `scripts/gen_language_data.py` and `scripts/gen_ui_strings.py` (re-runs only
translate what is missing). Seven languages Sarvam rates as weaker are marked "limited". Direction and font follow
the script of the text: Urdu and Kashmiri are right to left in Nastaliq, Sindhi comes back in Devanagari.

## Look

Cool paper, one steel-blue, and a clay red used only for what changed. Three pen marks carry meaning: a tick for an
answer that held, a wave for one that wobbled, a strike for one that changed. Type is Anek, one family drawn for Latin and
nine Indian scripts. Every text colour passes WCAG AA (ratios computed, not assumed). The logo is a lowercase v with
one dot, like the dot that marks a nasal sound in Devanagari and Telugu.

## Public-deployment safety

Visitors can bring their own Sarvam key (sent only with their own requests); otherwise a per-visitor daily allowance
and a per-minute limit apply. Reports are signed, so `/api/explain` is not a free relay. Custom endpoints are off on a
hosted deployment; metadata and link-local addresses are always refused. A strict Content-Security-Policy. Row-level
security keeps saved audits private to their owner.
