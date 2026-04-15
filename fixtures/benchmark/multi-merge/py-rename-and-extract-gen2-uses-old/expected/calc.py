def _sum_loop(items):
    s = 0
    for x in items:
        s = s + x
    return s


def sum_all(items):
    return _sum_loop(items)


def avg(items):
    return sum_all(items) / max(len(items), 1)
