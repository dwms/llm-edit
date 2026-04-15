// consumers of user factories live here
export function guestConsumer() {
  return buildUser("guest");
}

export function pairConsumer() {
  return [buildUser("a"), buildUser("b")];
}
