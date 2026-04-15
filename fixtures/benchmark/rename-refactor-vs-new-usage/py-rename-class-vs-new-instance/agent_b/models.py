class UserAccount:
    def __init__(self, email):
        self.email = email


def make_guest():
    return UserAccount("guest@example.com")


def make_admin(email):
    acct = UserAccount(email)
    acct.is_admin = True
    return acct
