def normalize(x):
    return x.strip().lower()


def run(xs):
    return [normalize(x) for x in xs]
