def fetch_data(key):
    raw = backend.get(key)
    return parse(raw)


def parse(raw):
    return raw.strip()
