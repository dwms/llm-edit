# js/const-vs-reassignment

## Scenario
A factory that returns a counter object. The local `count` variable is declared `let` but, at the time, never reassigned.

## Agent A's intent
Tighten the declaration from `let` to `const` because static analysis showed no reassignment.

## Agent B's intent
Add an `inc()` method on the returned object that increments `count` — turning the previously-unused-for-reassignment local into a genuinely mutable binding.

## Why text-diff merge fails
A modifies line 2 (`let` → `const`). B appends a method on a later line. The edits don't textually overlap — diff3 merges cleanly and silently produces:

```javascript
function newCounter() {
  const count = 0;
  return {
    get: () => count,
    inc: () => { count = count + 1; },  // TypeError: assignment to constant
  };
}
```

Shipped to production, `inc()` throws at runtime.

## Why a human also struggles
A reviewer looking at either diff in isolation sees a reasonable change. Only by reading A's and B's diffs together and tracing the `count` binding does the contradiction surface.

## Why structured op-logs make this trivial
- Agent A's op: `retype_binding(count, let→const)` with precondition `count.reassigned == false`.
- Agent B's op: `add_method(inc, body=..., mutates=[count])`.
- Merger: B's op invalidates A's precondition → drop A's retype and warn. Or: compose, notice the conflict, ask for human resolution with the actual semantic conflict surfaced (not two disjoint text hunks).
