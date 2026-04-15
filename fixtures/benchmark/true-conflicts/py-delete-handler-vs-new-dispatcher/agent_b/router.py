def handle_get(req):
    return req


def handle_post(req):
    return req


def dispatch(req):
    if req.method == "GET":
        return handle_get(req)
    if req.method == "POST":
        return handle_post(req)
    return None
