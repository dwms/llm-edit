export interface PurchaseOrder {
  id: string;
  total: number;
}

export function orderSummary(o: PurchaseOrder): string {
  return `${o.id}: $${o.total}`;
}
