# py-delete-init-vs-new-bootstrap

A removes the `init_state()` factory. B adds a `bootstrap()` helper that
calls `init_state()` to seed module state. Diff3 silently merges:
deletion at file top + insertion at file bottom never collide. Result
is a file with a dangling reference. Structured merge raises
DeleteVsNewCaller.

diff3 failure mode: **silent-clean-on-negative**.
