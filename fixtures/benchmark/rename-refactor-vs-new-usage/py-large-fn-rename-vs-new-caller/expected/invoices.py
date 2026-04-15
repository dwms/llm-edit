def calculate_total(invoice):
    subtotal = 0
    for line in invoice.lines:
        if line.skip:
            continue
        qty = line.quantity
        price = line.unit_price
        row_total = qty * price
        if line.discount_pct:
            row_total = row_total * (1 - line.discount_pct / 100)
        subtotal += row_total
    tax = 0
    for jurisdiction in invoice.taxes:
        rate = jurisdiction.rate
        taxable = subtotal
        if jurisdiction.exempt_lines:
            for exempt in jurisdiction.exempt_lines:
                taxable -= exempt.amount
        tax += taxable * rate
    total = subtotal + tax
    if invoice.rounding == "nearest-cent":
        total = round(total, 2)
    elif invoice.rounding == "nearest-dollar":
        total = round(total)
    invoice.total_cache = total
    return total


def format_invoice(invoice):
    return f"Invoice {invoice.id}: ${calculate_total(invoice):.2f}"


def total_for_report(invoice):
    return calculate_total(invoice) * 1.0
