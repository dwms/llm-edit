def load(key):
    return cache.get(key)


def evict(key):
    cache.pop(key, None)


def refresh(key):
    value = load(key)
    if value is None:
        evict(key)
    return value
