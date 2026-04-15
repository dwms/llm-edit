def process(items):
    total = 0
    for item in items:
        total = total + item.price * item.quantity

    # all done
    return total
