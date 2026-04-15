class Account:
    def __init__(self, email):
        self.email = email


def make_guest():
    return Account("guest@example.com")
