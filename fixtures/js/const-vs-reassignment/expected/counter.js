function newCounter() {
  let count = 0;
  return {
    get: () => count,
    inc: () => { count = count + 1; },
  };
}
