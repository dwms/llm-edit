def encode(x):
    return bytes(str(x), "utf-8")


def serialize(obj):
    return str(obj)


def decode(b):
    return b.decode("utf-8")
