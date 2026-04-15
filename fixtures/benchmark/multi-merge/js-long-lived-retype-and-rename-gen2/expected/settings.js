export function getDefaults() {
  const timeout = 30;
  const retries = 3;
  return { timeout, retries };
}

export function describe() {
  return "using " + JSON.stringify(getDefaults());
}
