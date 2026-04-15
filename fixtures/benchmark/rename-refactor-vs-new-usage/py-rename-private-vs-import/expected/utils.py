def _clean(s):
    return s.strip().lower()


def canonical(s):
    return _clean(s)


def canonical_list(items):
    return [_clean(i) for i in items]
