import asyncio
import collections
import functools
import hashlib
import json
import logging
import os
import time


def setup():
    logging.basicConfig(level=logging.INFO)
    return {
        "pid": os.getpid(),
        "started_at": time.time(),
        "queue": collections.deque(),
    }


def fingerprint(payload):
    return hashlib.sha256(json.dumps(payload).encode()).hexdigest()


async def run():
    state = setup()
    while True:
        await asyncio.sleep(1)
        state["queue"].append(time.time())
