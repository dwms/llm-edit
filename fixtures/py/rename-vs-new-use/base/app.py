from db import query


def get_user(user_id):
    return query("SELECT * FROM users WHERE id = ?", [user_id])


def send_welcome(user_id):
    user = get_user(user_id)
    print(f"Welcome, user {user_id}")
    return user


def delete_user(user_id):
    query("DELETE FROM users WHERE id = ?", [user_id])
