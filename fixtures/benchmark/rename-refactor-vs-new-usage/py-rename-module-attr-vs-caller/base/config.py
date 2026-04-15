DEFAULT_TIMEOUT = 30


def with_timeout(t):
    return min(t, DEFAULT_TIMEOUT)
