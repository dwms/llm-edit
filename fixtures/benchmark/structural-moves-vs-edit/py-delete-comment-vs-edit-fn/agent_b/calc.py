def total(items):
    # TODO: support discounts
    return sum(i.price * (1 - i.discount) for i in items)
