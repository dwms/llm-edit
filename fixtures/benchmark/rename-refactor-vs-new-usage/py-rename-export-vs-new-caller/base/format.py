def format_money(n):
    return "$" + str(round(n, 2))


def render_line(item):
    return item.name + ": " + format_money(item.price)
