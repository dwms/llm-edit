function fetchUser(id) {
  return db.users.get(id);
}

function listUsers() {
  return db.users.all();
}

function logAndFetch(id) {
  console.log(`calling fetchUser(${id})`);
  return fetchUser(id);
}
