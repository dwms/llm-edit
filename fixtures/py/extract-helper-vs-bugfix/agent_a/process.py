def _subtotal(items):
    total = 0
    for item in items:
        if item.valid:
            price = item.price * item.quantity
            total += price
    return total


def process(items):
    return _subtotal(items)
