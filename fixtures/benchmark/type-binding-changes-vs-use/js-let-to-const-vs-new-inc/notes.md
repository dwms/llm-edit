# js-let-to-const-vs-new-inc
A tightens 'let subtotal' to 'const' (precondition: no reassignments in
base). B concurrently adds an `add(p)` method that reassigns subtotal.
B's op invalidates A's precondition. Correct merge: drop A's retype and
emit a warning (or surface a semantic conflict); text diff merges silently
into broken code.
