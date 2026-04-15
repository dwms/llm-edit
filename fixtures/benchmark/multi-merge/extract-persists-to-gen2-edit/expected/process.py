def _subtotal(items):
    total = 0
    for item in items:
        total += item.price * 1.08
    return total


def process(items):
    return _subtotal(items)
