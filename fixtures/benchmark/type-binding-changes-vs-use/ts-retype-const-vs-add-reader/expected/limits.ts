export function limits() {
  const maxRetries = 5;
  const doubled = maxRetries * 2;
  return { maxRetries };
}
