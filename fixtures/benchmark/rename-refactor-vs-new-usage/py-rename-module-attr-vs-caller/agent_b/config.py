DEFAULT_TIMEOUT = 30


def with_timeout(t):
    return min(t, DEFAULT_TIMEOUT)


def double_timeout():
    return DEFAULT_TIMEOUT * 2
