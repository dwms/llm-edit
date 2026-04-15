def canonicalize(x):
    return x.strip().lower()


def run(xs):
    return [canonicalize(x) for x in xs]


def run_grouped(groups):
    return {
        key: [canonicalize(v) for v in (canonicalize(w) for w in items) if canonicalize(v)]
        for key, items in groups.items()
    }
