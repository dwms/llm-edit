def start_server(host, port):
    print("starting", host, port)


def stop_server():
    print("stopping")


def restart_server():
    stop_server()


def supervise(host, port):
    # wraps start_server with logging
    print(f"calling start_server({host}, {port})")
    start_server(host, port)
