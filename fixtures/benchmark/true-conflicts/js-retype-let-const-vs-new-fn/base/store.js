function makeStore() {
  let value = null;
  return {
    read: () => value,
  };
}
