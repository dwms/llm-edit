def new_api(x):
    return x + 1


def batch(xs):
    return [new_api(x) for x in xs]
