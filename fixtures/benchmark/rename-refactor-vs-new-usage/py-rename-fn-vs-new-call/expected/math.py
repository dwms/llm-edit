def sqrt(n):
    return n ** 0.5


def hypot(a, b):
    return sqrt(a * a + b * b)


def magnitude(v):
    return sqrt(v.x * v.x + v.y * v.y + v.z * v.z)
