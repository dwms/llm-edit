def run(items):
    results = []
    for it in items:
        x = it.value
        print("item", x)
        results.append(x * 2)
    return results
