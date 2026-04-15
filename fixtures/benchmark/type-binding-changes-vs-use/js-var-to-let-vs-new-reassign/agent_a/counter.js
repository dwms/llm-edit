function newCounter() {
  let count = 0;
  return {
    read: () => count,
  };
}
