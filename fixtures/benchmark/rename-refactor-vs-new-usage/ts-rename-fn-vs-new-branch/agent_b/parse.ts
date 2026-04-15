export function parseInput(s: string): number {
  return parseInt(s, 10);
}

export function handle(s: string): number {
  const n = parseInput(s);
  if (n < 0) return 0;
  return n;
}

export function handleSafe(s: string | undefined): number {
  if (s === undefined) return 0;
  return parseInput(s);
}
