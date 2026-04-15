import os


def collect():
    return os.environ.copy()


def aggregate():
    return collect()
