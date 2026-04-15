function makeFlag() {
  let on = false;
  return {
    is: () => on,
  };
}
