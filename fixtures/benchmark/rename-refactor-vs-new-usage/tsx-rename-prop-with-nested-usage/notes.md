# tsx-rename-prop-with-nested-usage

A renames parameter `userName -> accountName` project-scope. B adds a
`salute()` function whose body references the old name in five places:
parameter, template literal (twice), and three call-site arguments to
`greet`/`farewell`. Regions don't overlap, so diff3 produces a clean
merge whose new function is still wired to the old name.

diff3 failure mode: **silent-wrong**.
