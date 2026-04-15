def get_user(uid):
    return _db.query("SELECT * FROM users WHERE id = ?", uid)


def create_user(email, name):
    row = _db.insert("users", {"email": email, "name": name})
    _audit("create_user", row.id)
    return row


def update_user(uid, **fields):
    _db.update("users", uid, fields)
    _audit("update_user", uid)
    return get_user(uid)


def delete_user(uid):
    _db.delete("users", uid)
    _audit("delete_user", uid)


def list_users(limit=100):
    return _db.query("SELECT * FROM users LIMIT ?", limit)


def count_users():
    return _db.query_scalar("SELECT COUNT(*) FROM users")


def _audit(action, uid):
    _db.insert("audit", {"action": action, "user_id": uid})
