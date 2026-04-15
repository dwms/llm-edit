# py-both-rename-same-symbol
Both agents independently rename get_conn -> connect. Structured merge
notices the ops are identical and applies once (idempotent). diff3 sees
the same post-image on both sides and merges cleanly — this positive
case counts as CORRECT for both tools. Included to ensure the merger
doesn't double-apply and corrupt the file.
