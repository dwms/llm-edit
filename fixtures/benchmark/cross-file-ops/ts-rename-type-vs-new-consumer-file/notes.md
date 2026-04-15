# ts-rename-type-vs-new-consumer-file

A renames `makeUser -> buildUser` project-scope (touches types.ts only
in this corpus). B adds two new consumer functions in the sibling
`consumers.ts` file that both reference the old name. diff3 merges each
file independently — no conflict in either — but the consumer file ends
up with three references to the dead `makeUser`. Structured merge
propagates the project rename into B's insert_after content.

diff3 failure mode: **silent-wrong**.
