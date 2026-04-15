function newCounter() {
  var count = 0;
  return {
    read: () => count,
  };
}
