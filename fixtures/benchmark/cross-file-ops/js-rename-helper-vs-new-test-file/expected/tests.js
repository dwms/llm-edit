// test suite placeholder
test("toIsoString returns ISO", () => {
  expect(toIsoString(new Date(0))).toBe("1970-01-01T00:00:00.000Z");
});

test("toIsoString handles now", () => {
  expect(typeof toIsoString(new Date())).toBe("string");
});
