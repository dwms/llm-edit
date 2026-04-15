def normalize(x):
    return x.strip().lower()


def run(xs):
    return [normalize(x) for x in xs]


def run_grouped(groups):
    return {
        key: [normalize(v) for v in (normalize(w) for w in items) if normalize(v)]
        for key, items in groups.items()
    }
