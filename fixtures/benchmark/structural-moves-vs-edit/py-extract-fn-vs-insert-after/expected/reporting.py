def _active_users(users):
    active = []
    for u in users:
        if u.active:
            active.append(u)
            print(u.id)
    return active


def report(users):
    return _active_users(users)
