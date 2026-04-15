# js-let-to-const-vs-add-reader
A tightens apiUrl to const. B adds a path method that only READS apiUrl
(no reassigns). Precondition holds; both apply. diff3 fails because A's
edit touches the declaration line and B's edit touches later lines in the
same tightly-formatted object -- context confusion.
