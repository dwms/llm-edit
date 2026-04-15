def old_get(k):
    return k


def stay():
    return 0


def adapter(k):
    return old_get(k) or "missing"
