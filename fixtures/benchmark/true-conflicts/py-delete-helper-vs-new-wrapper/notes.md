# py-delete-helper-vs-new-wrapper

A deletes `helper(x)` from util.py. B adds a new `wrapped(x)` whose body
calls `helper(x) + 10`. The delete and add hunks are far apart, so diff3
silently merges into a file containing `wrapped` calling a function that
no longer exists. Structured merge detects DeleteVsNewCaller.

diff3 failure mode: **silent-clean-on-negative**.
