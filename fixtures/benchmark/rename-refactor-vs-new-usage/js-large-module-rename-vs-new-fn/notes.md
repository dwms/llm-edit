# js-large-module-rename-vs-new-fn

A 30+ line JS module with four exports and internal state. A renames `drain → flush` module-scope, updating the declaration, one internal call site, and the CommonJS `module.exports`. B adds `peekOrDrain()` (note the mixed-case keeps the word-boundary rename from over-matching) that calls `drain()`.

After merge, the internal `drain()` call in the new fn should become `flush()`, but the camelCase `Drain` in the new function's own name must stay untouched. Tests word-boundary correctness on a realistic-size JS file.
