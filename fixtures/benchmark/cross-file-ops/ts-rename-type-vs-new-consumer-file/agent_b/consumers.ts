// consumers of user factories live here
export function guestConsumer() {
  return makeUser("guest");
}

export function pairConsumer() {
  return [makeUser("a"), makeUser("b")];
}
