def get_user(user_id):
    return user_id


def delete_user(user_id):
    query("DELETE FROM users WHERE id = ?", [user_id])
