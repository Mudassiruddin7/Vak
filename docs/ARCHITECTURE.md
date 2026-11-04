# How Vak is put together

```
web/                 plain ES modules and CSS, no build step; embedded into the server binary
crates/
  vak-core/          pure logic, no I/O: text and script handling, the 23-language registry, writing
                     rules and their checker, perturbations, scoring and flags, effort and difficulty,
                     the live-interpreter trust score
  vak-app/           use cases behind traits: audit engine, judge, explainer, planner, live turns
  vak-adapters/      Sarvam clients (rate limit, retries, cache), custom endpoint with SSRF guard, Supabase
  vak-server/        axum: streaming audits (SSE), live turns, voice, quotas, security headers, static files
supabase/migrations  the two tables and their row-level-security policies
```

Dependencies point inward: the server knows the adapters, the adapters know the use cases, and the core
knows nothing about any of them. Every use case is tested against scripted fakes of the network.

## Why Rust here

An audit is mostly waiting on the network, so Rust does not make a single call faster. What it buys is a
small static binary that starts in milliseconds (cold starts on serverless), a few MB of memory per
instance, cheap concurrency for the dozens of calls in one audit, and a type system that keeps the report
shape, the flag codes and the wording banks in step (a test fails if a language is missing a flag text).

## Fewer calls, fewer tokens

* **Two waves.** The first wave runs one test of each kind. A kind only gets its remaining tests when the
  first came back less than clean. A steady model is cleared with a handful of calls.
* **Effort levels** (Light, Balanced, Thorough) and an **Auto** mode that reads the prompt's difficulty
  (length, numbers, mixed scripts, conditions, several questions) and picks one.
* One helper call writes the rewordings and the meaning-flipped versions together.
* The judge replies with one letter per answer, plus a reason only where something differs.
* Obvious matches (same text, same numbers) never reach the judge.
* Plain pasted text skips the planner call entirely.
* Deterministic calls are cached in memory for an hour. Nothing is written to disk.

## Public-deployment safety

* Visitors can bring their own Sarvam key (sent only with their own requests, never stored); otherwise a
  per-visitor daily allowance and a per-minute limit apply.
* Reports are signed, so `/api/explain` cannot be used as a free language-model relay.
* Custom endpoints are off on a hosted deployment; when on, metadata and link-local addresses are always
  refused and the connection is pinned to the address that was checked.
* A strict Content-Security-Policy (no inline scripts, same-origin only plus the Supabase project).
* Row-level security keeps saved audits private to their owner.
