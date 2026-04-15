def load_data(key):
    raw = backend.get(key)
    return strip_raw(raw)


def strip_raw(raw):
    return raw.strip()


def warm_cache(keys):
    return [load_data(k) for k in keys]
