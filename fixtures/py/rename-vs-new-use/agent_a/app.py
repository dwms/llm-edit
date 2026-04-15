from db import query


def get_user(account_id):
    return query("SELECT * FROM users WHERE id = ?", [account_id])


def send_welcome(account_id):
    user = get_user(account_id)
    print(f"Welcome, user {account_id}")
    return user


def delete_user(account_id):
    query("DELETE FROM users WHERE id = ?", [account_id])
