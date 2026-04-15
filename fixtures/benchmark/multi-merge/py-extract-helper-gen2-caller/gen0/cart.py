def total(items):
    subtotal = 0
    for item in items:
        subtotal += item.price
    return subtotal
