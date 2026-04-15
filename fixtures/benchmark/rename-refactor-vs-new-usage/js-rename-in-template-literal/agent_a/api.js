function loadUser(id) {
  return db.users.get(id);
}

function listUsers() {
  return db.users.all();
}
