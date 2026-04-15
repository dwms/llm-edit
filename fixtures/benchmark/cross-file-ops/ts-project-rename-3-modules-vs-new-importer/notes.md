# ts-project-rename-3-modules-vs-new-importer

A renames `makeStore → newStore` project-scope across core.ts, cache.ts,
queue.ts (3 modules touched). B adds `registerAll()` in registry.ts
referencing the dead name three times. diff3 sees no conflict in
registry.ts and ships the new function with three dead calls.
Structured merge propagates the project rename into B's insert_after
content.

diff3 failure mode: **silent-wrong**.
