def _normalize(s):
    return s.strip().lower()


def canonical(s):
    return _normalize(s)


def canonical_list(items):
    return [_normalize(i) for i in items]
