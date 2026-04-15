def _double_items(items):
    results = []
    for it in items:
        x = it.value
        results.append(x * 2)
    return results


def run(items):
    return _double_items(items)
