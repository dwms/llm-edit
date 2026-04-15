# py-delete-old-api-vs-new-adapter

A removes the deprecated `old_get`. B introduces an `adapter` function
that wraps `old_get`. Diff3 silently merges (regions are far apart);
the result file contains a dangling reference. Structured merge raises
DeleteVsNewCaller.

diff3 failure mode: **silent-clean-on-negative**.
