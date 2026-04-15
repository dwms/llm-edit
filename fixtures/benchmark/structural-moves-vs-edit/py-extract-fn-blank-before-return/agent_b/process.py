def process(items):
    total = 0
    for item in items:
        total = total + item.price * item.quantity * 1.08

    # all done
    return total
