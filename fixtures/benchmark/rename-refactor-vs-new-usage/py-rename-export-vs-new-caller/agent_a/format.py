def format_currency(n):
    return "$" + str(round(n, 2))


def render_line(item):
    return item.name + ": " + format_currency(item.price)
