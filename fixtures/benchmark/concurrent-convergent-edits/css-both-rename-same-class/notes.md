# css-both-rename-same-class
Both agents rename .btn-old -> .btn-new. Convergent refactor; merger must
not duplicate (applying A then B would no-op but applying B's rename atop
A's already-renamed tree must tolerate that the old name is gone).
