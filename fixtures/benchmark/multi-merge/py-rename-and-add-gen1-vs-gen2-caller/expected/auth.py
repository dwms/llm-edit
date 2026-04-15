def verify_token(token):
    return token == SECRET


def revoke_token(token):
    revoked.add(token)


def rotate_secret(new_secret):
    global SECRET
    SECRET = new_secret


def require_auth(token):
    if not verify_token(token):
        raise PermissionError()
    return True
