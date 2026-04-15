# py-extract-fn-vs-replace-inside
A extracts the loop into _subtotal. B edits the loop body inside process.
Merged by line diff, B's edit is lost (process no longer contains the line)
or duplicated. Proper merge: re-route B's edit into the extracted helper.
