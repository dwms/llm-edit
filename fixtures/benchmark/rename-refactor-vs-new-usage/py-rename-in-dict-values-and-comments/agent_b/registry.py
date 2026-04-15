def ping_host(h):
    return h


def pong_host(h):
    return h


def build_dispatch():
    # Dispatch table: each entry binds a verb to a handler.
    # ping_host handles reachability checks.
    return {
        "ping": ping_host,
        "pong": pong_host,
        "meta": {"default": ping_host, "fallback": pong_host},
    }
