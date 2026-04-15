# delete-comment-survives-prepend

Regression for issue #7: positional `{kind: "comment", index: 0}` anchors
drift when a concurrent edit prepends a sibling comment.

- **Base**: `total` has one comment, `# TODO: support discounts`.
- **Agent A**: deletes that comment. Records a `delete_node` op whose
  anchor uses a content-hash `node_id` (`cf2f36a8b227`) derived from
  `(parent_fn, normalized_text)` — not a positional index.
- **Agent B**: prepends an unrelated `# new note` before the TODO line.
- **Merge**: applies B's insert first (or vice versa — symmetric), then A's
  delete, which resolves its `node_id` anchor against the current tree by
  walking `total`'s body for a comment whose normalized text matches.
  The new comment survives; the originally-targeted comment is removed.

A positional `{kind: "comment", index: 0}` anchor would have deleted the
*new* comment instead, losing B's intent. The content-hash anchor makes A
and B commute.
