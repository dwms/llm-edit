# multi-merge/css-rename-persists-to-gen2-new-element

Gen 1 renamed `.card` → `.panel` project-wide. Gen 2 added a new element with `class="card"`. With persistence, the gen-1 rename propagates into gen-2's insert payload. Without persistence, the new element is inserted with the orphaned class.

(Note: the "A" element's gen-1 class is `card` because `gen1_right.jsonl` is empty, but the merge applies the rename → "A" becomes `panel`. The gen-2 addition `"B"` should also get the rename applied.)
