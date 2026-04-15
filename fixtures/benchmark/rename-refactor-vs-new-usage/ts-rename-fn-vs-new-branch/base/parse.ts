export function parseInput(s: string): number {
  return parseInt(s, 10);
}

export function handle(s: string): number {
  const n = parseInput(s);
  if (n < 0) return 0;
  return n;
}
