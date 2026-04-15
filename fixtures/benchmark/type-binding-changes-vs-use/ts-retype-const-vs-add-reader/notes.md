# ts-retype-const-vs-add-reader

A retypes `let maxRetries → const maxRetries` (precondition `reassigned: false`). B adds two read-only usages of `maxRetries` via replace_in_anchor — they don't reassign, so the precondition stays valid and the merge should succeed.
