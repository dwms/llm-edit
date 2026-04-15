def format_currency(n):
    return "$" + str(round(n, 2))


def render_line(item):
    return item.name + ": " + format_currency(item.price)


def render_total(items):
    total = sum(i.price for i in items)
    return "Total: " + format_currency(total)
