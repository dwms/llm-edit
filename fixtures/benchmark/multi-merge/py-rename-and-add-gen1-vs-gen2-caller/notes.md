# py-rename-and-add-gen1-vs-gen2-caller

- Gen 0: `check_token`, `revoke_token`.
- Gen 1 left: rename `check_token → verify_token`.
- Gen 1 right: independently adds `rotate_secret` after `revoke_token`.
- Gen 2: adds `require_auth` (inserted after `rotate_secret`) that calls `check_token`.

Persistence matters on two axes: (1) gen 2's anchor `rotate_secret` was created in gen 1 right, so the gen 1 merged state must have it, and (2) gen 2's new body references `check_token` which gen 1 left renamed. Both gen-1 logs must survive into the gen-2 merge.
