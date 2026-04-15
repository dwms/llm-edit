function makeBox() {
  let inner = "x";
  return {
    peek: () => inner,
  };
}
