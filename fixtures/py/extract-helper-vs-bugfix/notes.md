# py/extract-helper-vs-bugfix

## Scenario
`process()` iterates items and sums their price. Two agents work in parallel.

## Agent A's intent
Extract the summation loop into a private `_subtotal()` helper; rewrite `process()` to delegate.

## Agent B's intent
Fix a bug: prices weren't being multiplied by the tax factor (1.08).

## Why text-diff merge fails
Both agents rewrite the body of `process()`, so diff3 sees two edits to the same hunk and emits a full conflict. The conflict spans most of the function.

## Why a human also struggles
Given the three texts, resolving correctly requires the human to:
1. Recognise that A *moved* the summation loop unchanged into `_subtotal()`.
2. Re-apply B's one-character-ish bug-fix (`* 1.08`) inside the relocated loop.
3. Keep A's new `process()` body (`return _subtotal(items)`), not B's.

Under "take both" instinct, the natural resolution keeps two copies of the loop (one in `_subtotal`, one still in `process`), or keeps the wrong copy and loses the bug fix.

## Why structured op-logs make this trivial
- Agent A's op: `extract_function(source=process, lines=3..7, new_name=_subtotal)`.
- Agent B's op: `replace_in_range(lines=6, old="* item.quantity", new="* item.quantity * 1.08")`.
- Merger: resolve B's range against A's post-extract line numbers → B's patch lands inside `_subtotal`.
