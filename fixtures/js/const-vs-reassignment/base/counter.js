function newCounter() {
  let count = 0;
  return {
    get: () => count,
  };
}
