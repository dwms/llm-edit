# css-rename-vs-variant-in-sibling-file
A renames .banner -> .hero-banner across the project. B adds .banner.sale
variant in a sibling stylesheet. Per-file text merge keeps B's unrenamed
new selector; result is an orphan that targets no element.
