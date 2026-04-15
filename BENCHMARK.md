# llm-edit benchmark — measurement spec

## Goal

Demonstrate that structured-op merging (`llm-edit merge`) resolves a
materially higher fraction of two-agent merges correctly than text-based
merging (`git merge-file` / `diff3 -m`).

## Target

On a curated benchmark corpus of **≥50 realistic cases**, llm-edit's
correctness rate shall exceed the diff3 baseline by **≥ 80 absolute
percentage points**.

Example passing runs:
- baseline 15% → llm-edit ≥ 95%
- baseline 5%  → llm-edit ≥ 85%
- baseline 20% → llm-edit ≥ 100% (perfect on the remaining 80)

## Scoring

Each case contributes one of three outcomes to each tool:

- **CORRECT** — merged output equals `expected/` byte-exactly (positive
  cases), or emitted conflict matches `conflict.txt` (negative cases).
- **WRONG** — merged output differs from expected (positive), or conflict
  missing/mismatched (negative). Silent-wrong merges count here.
- **CONFLICT-UNRESOLVED** — diff3 produces `<<<<<<<` markers and leaves
  the resolution to a human. Counts as WRONG — we are scoring automation.

The percentage for a tool is `CORRECT / total`.

## Corpus composition (target shape)

At least 50 cases, balanced across:

| Bucket | Count | Notes |
|---|---|---|
| rename / refactor vs new usage | ≥10 | Python, JS, TS, CSS |
| structural moves vs edit | ≥8 | extract fn, inline, move, wrap |
| ordering changes vs insert | ≥6 | sort imports, reorder block, then add |
| type/binding changes vs use | ≥4 | let→const invalidation, etc. |
| cross-file ops | ≥8 | CSS class rename → HTML + JSX, JS fn → handler |
| concurrent convergent edits | ≥6 | both fix same typo, etc. |
| true conflicts (negatives) | ≥8 | deliberately unresolvable |

Hand-written is fine; real-world-inspired is better. The corpus lives
under `fixtures/benchmark/` distinct from the design-illustration
fixtures under `fixtures/<bucket>/`.

## Harness

`cargo run --bin bench` walks `fixtures/benchmark/`, runs each case
through both tools, counts outcomes, prints a table, and exits 0 iff the
80-point gap is met.

## Discipline

- Cases in the benchmark must NOT be authored by tweaking the merger to
  pass them. Author a batch, freeze it, measure, then improve the merger
  until it passes. No test-driven corpus hacking.
- Negative (true-conflict) cases are part of the benchmark: a tool that
  silently merges a genuine conflict scores WRONG, not CORRECT.
