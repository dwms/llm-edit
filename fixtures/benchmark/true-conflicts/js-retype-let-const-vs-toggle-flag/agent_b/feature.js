function makeFlag() {
  let on = false;
  return {
    is: () => on,
    toggle: () => { on = !on; },
  };
}
