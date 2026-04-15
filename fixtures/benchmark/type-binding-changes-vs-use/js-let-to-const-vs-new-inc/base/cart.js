function makeCart() {
  let subtotal = 0;
  return {
    get: () => subtotal,
  };
}
