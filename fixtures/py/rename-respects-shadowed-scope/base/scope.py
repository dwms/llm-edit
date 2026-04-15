def outer(user_id):
    def inner(user_id):
        return user_id
    return user_id + inner(user_id)
