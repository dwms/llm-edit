export interface Order {
  id: string;
  total: number;
}

export function orderSummary(o: Order): string {
  return `${o.id}: $${o.total}`;
}
