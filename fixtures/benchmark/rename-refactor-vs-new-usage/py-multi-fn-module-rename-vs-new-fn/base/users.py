def get_user(user_id):
    return _db.query("SELECT * FROM users WHERE id = ?", user_id)


def create_user(email, name):
    row = _db.insert("users", {"email": email, "name": name})
    _audit("create_user", row.id)
    return row


def update_user(user_id, **fields):
    _db.update("users", user_id, fields)
    _audit("update_user", user_id)
    return get_user(user_id)


def delete_user(user_id):
    _db.delete("users", user_id)
    _audit("delete_user", user_id)


def list_users(limit=100):
    return _db.query("SELECT * FROM users LIMIT ?", limit)


def count_users():
    return _db.query_scalar("SELECT COUNT(*) FROM users")


def _audit(action, user_id):
    _db.insert("audit", {"action": action, "user_id": user_id})
