# py/rename-vs-new-use

## Scenario
Common Python module with functions taking `user_id`. Two agents work concurrently in separate worktrees.

## Agent A's intent
Rename `user_id` → `account_id` throughout the module for terminology consistency.

## Agent B's intent
Add a new `ban_user` function that follows the existing convention.

## Why text-diff merge fails
`diff3 -m` produces conflict markers because A's rename of `delete_user`'s parameter falls in the same hunk that B extended by appending `ban_user`. The conflict output looks like:

```
<<<<<<< agent_a
def delete_user(account_id):           # A's rename
    query(..., [account_id])
=======
def delete_user(user_id):              # B's unchanged copy
    query(..., [user_id])

def ban_user(user_id, reason):         # B's new function
    ...
>>>>>>> agent_b
```

## Why a human also struggles
The "obvious" resolution is to take both sides — which yields two problems at once:
1. Duplicate `delete_user` definition (A's renamed version and B's original).
2. `ban_user` still references `user_id`, which no longer exists elsewhere in the file.

A careful human has to recognise that A's and B's `delete_user` are the same function modulo A's rename (keep A's, discard B's) *and* that A's rename must propagate into B's new `ban_user`. Two semantic steps, both easy to miss on a real-sized diff.

## Why structured op-logs make this trivial
- Agent A's op: `rename_symbol(user_id → account_id, scope=module)`
- Agent B's op: `add_function(ban_user, body=...)` — body references `user_id`
- Merger: apply A's rename to every identifier in B's additions. Done.
