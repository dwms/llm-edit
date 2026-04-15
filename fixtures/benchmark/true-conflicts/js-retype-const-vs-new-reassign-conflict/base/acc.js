function acc() {
  let total = 0;
  return {
    read: () => total,
  };
}
