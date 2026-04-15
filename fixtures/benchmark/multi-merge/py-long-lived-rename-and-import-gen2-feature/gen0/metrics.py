import time


def record(key, value):
    _store[key] = value


def flush():
    for k, v in _store.items():
        print(k, v)


_store = {}
