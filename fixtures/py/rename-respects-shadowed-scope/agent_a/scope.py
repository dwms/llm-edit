def outer(account_id):
    def inner(user_id):
        return user_id
    return account_id + inner(account_id)
