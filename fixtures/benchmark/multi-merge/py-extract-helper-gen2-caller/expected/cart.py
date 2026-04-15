def _subtotal(items):
    subtotal = 0
    for item in items:
        subtotal += item.price
    return subtotal


def total(items):
    return _subtotal(items)


def total_with_tax(items, rate):
    return total(items) * (1 + rate)
