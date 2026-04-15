def transform(rows):
    cleaned = []
    for row in rows:
        if row is None:
            continue
        cleaned.append(row.strip())
    return cleaned
