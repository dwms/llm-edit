# negative/py/conflicting-rename

Both agents renamed the same symbol but to different names. No merger should silently pick one; this is a genuine naming disagreement that requires human resolution.

Expected output: a single `rename_symbol_mismatch` conflict report.
