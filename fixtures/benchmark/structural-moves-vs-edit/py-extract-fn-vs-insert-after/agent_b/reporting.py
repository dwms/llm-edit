def report(users):
    active = []
    for u in users:
        if u.active:
            active.append(u)
            print(u.id)
    return active
