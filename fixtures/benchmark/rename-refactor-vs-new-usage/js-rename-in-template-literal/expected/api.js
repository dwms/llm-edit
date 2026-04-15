function loadUser(id) {
  return db.users.get(id);
}

function listUsers() {
  return db.users.all();
}

function logAndFetch(id) {
  console.log(`calling loadUser(${id})`);
  return loadUser(id);
}
