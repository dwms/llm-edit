# html-wrap-vs-css-rename
A renames .card -> .tile. B wraps the section children in a .grid div.
Text diff conflicts on the re-indented HTML; structured merge wraps and
then propagates A's rename to the now-indented children.
