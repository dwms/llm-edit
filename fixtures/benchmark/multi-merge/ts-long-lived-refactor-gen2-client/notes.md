# ts-long-lived-refactor-gen2-client

Long-lived branch pattern: gen 1 left contains a *chain* of refactors (two successive project renames), gen 1 right is idle, and gen 2 builds a feature on top that calls both renamed symbols under their old names plus introduces a new symbol (`httpPut`) that is *not* renamed.

With persistence, the gen-1 log carries *both* renames forward and gen 2's inserted body has `httpGet → get` and `httpPost → post`. Without persistence, both call sites in the inserted function break. Note `httpPut` stays `httpPut` — only pre-existing symbols get rewritten.
