import json
import os


def snapshot():
    return os.environ.copy()


def aggregate():
    return snapshot()


def export():
    data = snapshot()
    return json.dumps(data)
