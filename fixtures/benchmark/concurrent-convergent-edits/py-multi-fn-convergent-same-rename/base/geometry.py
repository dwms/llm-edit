def dist(a, b):
    dx = a.x - b.x
    dy = a.y - b.y
    return (dx * dx + dy * dy) ** 0.5


def closest(points, target):
    best = None
    best_d = float("inf")
    for p in points:
        d = dist(p, target)
        if d < best_d:
            best_d = d
            best = p
    return best


def neighbors_within(points, target, radius):
    result = []
    for p in points:
        if dist(p, target) <= radius:
            result.append(p)
    return result


def farthest(points, target):
    best = None
    best_d = -1.0
    for p in points:
        d = dist(p, target)
        if d > best_d:
            best_d = d
            best = p
    return best
