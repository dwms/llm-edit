def _uppercase_all(tokens):
    result = []
    for t in tokens:
        result.append(t.upper())
    return result


def parse(tokens):
    return _uppercase_all(tokens)
