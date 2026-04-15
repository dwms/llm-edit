# py-long-lived-rename-chain-and-extract-gen2-caller

Long-lived branch pattern: gen 1 left renames `transform → clean_rows` **and** adds a new helper `count_rows` on top. Gen 2 then builds `report` that references both:
- `count_rows` (a gen-1 creation, requires persistence of the *add*).
- `transform` (the old name, requires persistence of the *rename*).

This fixture is one of the strongest arguments for gen-1 metadata persistence: gen 2's anchor (`after: count_rows`) only exists because of gen 1, *and* its body references a name gen 1 retired.
