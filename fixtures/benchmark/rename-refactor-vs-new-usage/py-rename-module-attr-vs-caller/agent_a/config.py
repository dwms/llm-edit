TIMEOUT_SECONDS = 30


def with_timeout(t):
    return min(t, TIMEOUT_SECONDS)
