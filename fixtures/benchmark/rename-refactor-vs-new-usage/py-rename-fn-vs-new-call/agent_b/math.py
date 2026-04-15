def square_root(n):
    return n ** 0.5


def hypot(a, b):
    return square_root(a * a + b * b)


def magnitude(v):
    return square_root(v.x * v.x + v.y * v.y + v.z * v.z)
