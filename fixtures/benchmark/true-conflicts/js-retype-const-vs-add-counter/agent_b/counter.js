function makeCounter() {
  let n = 0;
  return {
    get: () => n,
    incr: () => { n = n + 1; },
  };
}
