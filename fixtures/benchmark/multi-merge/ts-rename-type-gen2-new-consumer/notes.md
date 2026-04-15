# ts-rename-type-gen2-new-consumer

- Gen 0: `types.ts` defines `UserRecord`; `repo.ts` imports and uses it.
- Gen 1 left: project rename `UserRecord → User`.
- Gen 1 right: no-op.
- Gen 2: adds `lastUser()` that also takes/returns `UserRecord` — C worked off the pre-rename types.

With persistence the inserted content has both type references rewritten to `User`. TypeScript type identifiers appear twice per signature so this stresses word-boundary replacement in a single line.
