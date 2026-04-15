def probe_host(h):
    return h


def pong_host(h):
    return h


def build_dispatch():
    # Dispatch table: each entry binds a verb to a handler.
    # probe_host handles reachability checks.
    return {
        "ping": probe_host,
        "pong": pong_host,
        "meta": {"default": probe_host, "fallback": pong_host},
    }
