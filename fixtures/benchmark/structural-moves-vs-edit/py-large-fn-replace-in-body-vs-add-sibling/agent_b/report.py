def build_report(db, start, end):
    rows = []
    for table in ["orders", "shipments", "returns"]:
        result = db.query(
            f"SELECT * FROM {table} WHERE created_at BETWEEN ? AND ?",
            start,
            end,
        )
        for r in result:
            rows.append({
                "table": table,
                "id": r.id,
                "amount": r.amount,
                "created_at": r.created_at,
            })
    totals = {}
    for row in rows:
        totals.setdefault(row["table"], 0)
        totals[row["table"]] += row["amount"]
    summary = {
        "range_start": start,
        "range_end": end,
        "totals": totals,
        "row_count": len(rows),
    }
    return summary


def pretty_print(report):
    print(f"{report['range_start']} → {report['range_end']}: {report['row_count']} rows")


def export_csv(report, path):
    with open(path, "w") as f:
        for table, total in report["totals"].items():
            f.write(f"{table},{total}\n")
