def read_item(k):
    return k


def write_item(k, v):
    return v


def has_item(k):
    return True


def size():
    return 0


def copy_to(dst, k):
    v = read_item(k)
    write_item(k, v)
    return v
