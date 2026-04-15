def calc_tax(amount, rate):
    return amount * rate


def compute_total(subtotal, rate):
    tax = calc_tax(subtotal, rate)
    return subtotal + tax


def invoice_line(item):
    return compute_total(item.price, 0.08)


def quote_total(items, rate):
    subtotal = sum(i.price for i in items)
    tax = calc_tax(subtotal, rate)
    return subtotal + tax
