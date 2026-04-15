def publish(payload):
    log.append(payload)
    return True


def shutdown():
    return False


def healthcheck():
    return publish({"ping": 1})


def retry_emit(payload):
    for _ in range(3):
        if publish(payload):
            return True
    return False
