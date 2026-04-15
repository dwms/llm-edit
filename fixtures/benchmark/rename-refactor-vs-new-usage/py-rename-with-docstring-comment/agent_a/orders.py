def submit_order(cart, user):
    return {"cart": cart, "user": user}


def cancel_order(order_id):
    return {"cancelled": order_id}
