export interface PurchaseOrder {
  id: string;
  total: number;
}

export function orderSummary(o: PurchaseOrder): string {
  return `${o.id}: $${o.total}`;
}

export function isBigOrder(o: PurchaseOrder): boolean {
  return o.total > 1000;
}
