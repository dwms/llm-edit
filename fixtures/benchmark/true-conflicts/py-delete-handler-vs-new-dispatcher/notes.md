# py-delete-handler-vs-new-dispatcher

A deletes `handle_get` (assumed unused). B adds a `dispatch(req)` whose
body branches on req.method, calling `handle_get` for the GET branch
and `handle_post` for POST. Diff3 silently merges: A's removal and B's
addition are in disjoint regions of the file. Structured merge raises
DeleteVsNewCaller.

diff3 failure mode: **silent-clean-on-negative**.
