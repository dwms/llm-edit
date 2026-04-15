# py-delete-private-fn-vs-new-public-caller

A removes a private helper `_internal_step` (assuming nothing depends
on it). B adds a new public-ish helper `half_then` whose body calls
`_internal_step`. Diff3 sees no overlap, ships a tree where the new
function references the dead private name. Structured merge raises
DeleteVsNewCaller.

diff3 failure mode: **silent-clean-on-negative**.
