function makeCart() {
  const subtotal = 0;
  return {
    get: () => subtotal,
  };
}
