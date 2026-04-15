function makeBox() {
  let inner = "x";
  return {
    peek: () => inner,
    swap: (v) => { inner = v; },
  };
}
