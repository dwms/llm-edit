def compute_tax(amount, rate):
    return amount * rate


def compute_total(subtotal, rate):
    tax = compute_tax(subtotal, rate)
    return subtotal + tax


def invoice_line(item):
    return compute_total(item.price, 0.08)
