# py-multi-fn-convergent-same-rename

A multi-function geometry module where `dist` appears as one definition and three call sites across four functions. Both A and B rename `dist → distance` with the *same* rename_symbol op. Merger must produce a single clean rename; second op is a no-op per the idempotence rules.
