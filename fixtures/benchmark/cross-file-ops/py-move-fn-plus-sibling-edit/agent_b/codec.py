def encode(x):
    return bytes(str(x), "utf-8")


def decode(b):
    return b.decode("utf-8")
