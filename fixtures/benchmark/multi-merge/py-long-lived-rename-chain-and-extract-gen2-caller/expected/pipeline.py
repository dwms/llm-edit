def clean_rows(rows):
    cleaned = []
    for row in rows:
        if row is None:
            continue
        cleaned.append(row.strip())
    return cleaned


def count_rows(rows):
    return len(clean_rows(rows))


def report(rows):
    return {
        "total": count_rows(rows),
        "cleaned": clean_rows(rows),
    }
