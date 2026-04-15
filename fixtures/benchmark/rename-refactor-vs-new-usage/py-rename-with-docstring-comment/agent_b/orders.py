def place_order(cart, user):
    return {"cart": cart, "user": user}


def cancel_order(order_id):
    return {"cancelled": order_id}


def retry_order(cart, user, attempts):
    # Retry place_order up to `attempts` times before giving up.
    last = None
    for _ in range(attempts):
        last = place_order(cart, user)
        if last:
            return last
    return last
