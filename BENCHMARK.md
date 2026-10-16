# Benchmark methodology

## Reproduce

From the repository root, with Rust and `diff3` installed:

```sh
cargo run --locked --release --bin bench
cargo run --locked --release --bin token_savings
```

Recorded reports: [merge outcomes](docs/benchmarks/merge-results.txt) and
[token estimates](docs/benchmarks/token-savings.txt). These were reproduced
on macOS on 2026-10-15. System diff3 implementations can differ.

## Compatible single merges: the directly comparable subset

There are 81 positive single-merge cases with `base/`, `agent_a/`, `agent_b/`,
and `expected/`. llm-edit uses each branch's recorded operations; the baseline
runs `diff3 -m A base B` per file. A pass requires the complete output tree to
match expected bytes and no unresolved conflict. Missing-side files require
handling beyond the baseline's three-existing-files invocation and count as
baseline failures. This is not a full `git merge` benchmark.

llm-edit passes 81/81; diff3 passes 5/81. The remaining 76 cases are compatible
merges where recorded semantics supply information the text baseline lacks.
The corpus is deliberately curated for semantic challenges, not sampled from
production merges. No claim is made about a universal success rate.

## Token model

For each of those 76 cases, `token_savings` estimates an LLM repair step as:

`floor((text needing repair + complete expected source tree characters) / 4)`

The first term includes conflicted or wrong merged files. For missing-side
files it uses the larger branch version. Operation-log files are excluded.
Summing per-case estimates gives **12,218 tokens**, or **160** per additional
resolution after integer rounding. The deterministic llm-edit merge engine
makes no LLM calls, so those hypothetical repair tokens are avoided.

This is an input-plus-full-rewrite model, not measured tokenizer output,
provider billing, or an end-to-end agent cost comparison. It excludes prompts,
retries, operation recording, validation, and review. A resolver that produces
small patches may cost less. No savings are assigned to already-correct text
merges, genuine conflicts, or chained merges.

## Full-suite scoring

The complete 128-case suite contains:

- 81 compatible single merges, scored against expected output bytes.
- 22 genuine conflicts, where llm-edit must emit the expected structured
  conflict. The text baseline passes only if it refuses with conflict markers.
- 25 chained merges, where llm-edit must preserve operation history across
  generations. **The harness assigns diff3 zero for this category without
  running an equivalent chained Git workflow.**

The aggregate result is llm-edit 128/128 and diff3 5/128, a +96.1 percentage
point gap. Because of the chained-merge scoring convention, use the 81-case
subset for a direct automatic-resolution comparison. Passing true-conflict
cases means correctly reporting a conflict, not automatically resolving it.
The current regression gate requires an aggregate gap of at least 90 points.

## Verified examples

The README links three fixture scenarios checked with both `diff3 -m` and
`git merge-file -p`: rename versus new caller, cross-file CSS class rename
versus new HTML usage, and sorting versus inserting imports. Both text tools
conflict on the first and third. On the CSS example both accept `page.html`
but fail to propagate the renamed class. llm-edit matches the expected trees.
These checks do not establish that every regular or semantic merge tool fails.

Keep fixture expectations independently reviewable. Add realistic cases before
changing the merger, retain genuine conflicts, and inspect silent wrong merges
as well as conflict markers.
