def canonicalize(x):
    return x.strip().lower()


def run(xs):
    return [canonicalize(x) for x in xs]
