def validate(payload):
    return bool(payload)


def parse(payload):
    return dict(payload)


def run(payload):
    if not validate(payload):
        return None
    return parse(payload)
