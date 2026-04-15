import json
import os


def run():
    return os.getcwd()


def dump_state(state):
    return json.dumps(state)
