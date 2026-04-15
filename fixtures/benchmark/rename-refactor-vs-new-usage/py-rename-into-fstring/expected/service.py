def boot_server(host, port):
    print("starting", host, port)


def stop_server():
    print("stopping")


def restart_server():
    stop_server()


def supervise(host, port):
    # wraps boot_server with logging
    print(f"calling boot_server({host}, {port})")
    boot_server(host, port)
