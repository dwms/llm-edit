def parse(tokens):
    result = []
    for t in tokens:
        result.append(t.upper())
    return result


def parse_and_sort(tokens):
    return sorted(parse(tokens))
