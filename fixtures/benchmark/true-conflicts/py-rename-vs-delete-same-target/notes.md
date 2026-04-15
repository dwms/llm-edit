# py-rename-vs-delete-same-target
A renames helper -> aux. B deletes helper entirely. Intents incompatible;
after B's delete main() is broken anyway; after A's rename main() points
to aux which B removed. Either way requires human decision.
