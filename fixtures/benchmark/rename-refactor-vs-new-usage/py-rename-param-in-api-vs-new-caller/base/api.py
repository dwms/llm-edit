def fetch(endpoint, retries):
    for i in range(retries):
        try:
            return _http_get(endpoint)
        except Exception:
            continue
    return None


def _http_get(url):
    return url
