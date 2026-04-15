# ts-rename-interface-vs-new-usage

A renames the `Order` interface to `PurchaseOrder` (module-scope). B independently adds `isBigOrder(o: Order)` — an insert_after whose payload references the pre-rename type name.

After merge, the new function must use `PurchaseOrder`. Exercises A's rename rewriting B's insert_after content in TypeScript.
