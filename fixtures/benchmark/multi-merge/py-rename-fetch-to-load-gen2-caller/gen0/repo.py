def fetch(key):
    return cache.get(key)


def evict(key):
    cache.pop(key, None)
