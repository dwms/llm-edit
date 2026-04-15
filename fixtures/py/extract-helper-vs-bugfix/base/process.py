def process(items):
    # compute subtotal
    total = 0
    for item in items:
        if item.valid:
            price = item.price * item.quantity
            total += price
    return total
