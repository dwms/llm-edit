function makeStore() {
  let value = null;
  return {
    read: () => value,
    write: (v) => { value = v; },
  };
}
