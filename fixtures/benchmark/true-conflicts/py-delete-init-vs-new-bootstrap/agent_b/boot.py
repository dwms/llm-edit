def init_state():
    return {}


def shutdown():
    return None


def bootstrap():
    state = init_state()
    state["ready"] = True
    return state
