def fetch(endpoint, max_attempts):
    for i in range(max_attempts):
        try:
            return _http_get(endpoint)
        except Exception:
            continue
    return None


def _http_get(url):
    return url


def fetch_many(endpoints, max_attempts):
    return [fetch(e, max_attempts) for e in endpoints]
