# js-retype-const-vs-new-reassign-conflict
A wants const (precondition: no reassigns). B's new method reassigns total.
A and B are in direct semantic conflict: adopting A breaks B's code;
adopting B loses A's intent. Requires human judgement.
