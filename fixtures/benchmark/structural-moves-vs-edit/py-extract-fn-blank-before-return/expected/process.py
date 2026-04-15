def _subtotal(items):
    total = 0
    for item in items:
        total = total + item.price * item.quantity * 1.08

    return total


def process(items):
    return _subtotal(items)
