def check_token(token):
    return token == SECRET


def revoke_token(token):
    revoked.add(token)
