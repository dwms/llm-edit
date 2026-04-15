export function limits() {
  let maxRetries = 5;
  const doubled = maxRetries * 2;
  return { maxRetries };
}
