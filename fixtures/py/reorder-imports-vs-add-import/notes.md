# py/reorder-imports-vs-add-import

## Scenario
A Python module's imports are in insertion order (not sorted).

## Agent A's intent
Sort imports alphabetically.

## Agent B's intent
Add `import re` because it's now needed.

## Why text-diff merge fails
Both agents touch the same contiguous import block, so diff3 flags a conflict or produces a half-sorted result. The "take both" resolution keeps A's sorted block followed by B's `import re` appended at the bottom — partially sorted, still wrong.

## Why a human also struggles
A reviewer who doesn't notice A's intent ("sort") may accept the concatenation and ship a nearly-sorted import block. On a 40-line import section this is easy to miss in review.

## Why structured op-logs make this trivial
- Agent A's op: `sort_imports(module=main.py)`.
- Agent B's op: `add_import(module=re)`.
- These ops commute — apply both, the add-import lands in its sorted position automatically.
