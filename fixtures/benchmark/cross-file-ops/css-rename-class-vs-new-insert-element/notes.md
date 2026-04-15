# css-rename-class-vs-new-insert-element

A renames `.badge -> .chip` project-wide (touches styles.css, home.html,
profile.html). B inserts a new `<span class="badge">` into profile.html.
Each file's diff3 merge is clean (A and B touch different lines of
profile.html, neither conflicts with the other), producing a profile
page whose new span still says `class="badge"`. Structured merge
rewrites B's insert_element.html through the class-rename table.

diff3 failure mode: **silent-wrong**.
