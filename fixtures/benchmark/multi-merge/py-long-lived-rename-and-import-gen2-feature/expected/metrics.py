import sys
import time


def emit(key, value):
    _store[key] = value


def drain():
    for k, v in _store.items():
        print(k, v)


def timed_record(key, value):
    emit(key, value)
    drain()


_store = {}
