export function parseNumber(s: string): number {
  return parseInt(s, 10);
}

export function handle(s: string): number {
  const n = parseNumber(s);
  if (n < 0) return 0;
  return n;
}
