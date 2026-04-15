def total(items):
    return sum(i.price * (1 - i.discount) for i in items)
