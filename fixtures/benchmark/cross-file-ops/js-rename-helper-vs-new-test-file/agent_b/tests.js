// test suite placeholder
test("formatDate returns ISO", () => {
  expect(formatDate(new Date(0))).toBe("1970-01-01T00:00:00.000Z");
});

test("formatDate handles now", () => {
  expect(typeof formatDate(new Date())).toBe("string");
});
