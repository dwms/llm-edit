# py-both-add-same-import
Both agents independently add 'import sys'. diff3 merges cleanly because
the textual result is identical. Structured merge must dedupe the op and
insert sys only once — naive apply of both logs would produce two `import
sys` lines.
