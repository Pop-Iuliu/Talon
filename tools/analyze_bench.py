import csv
import math
import sys
from collections import defaultdict


def load(path):
    data = defaultdict(list)
    with open(path) as f:
        for row in csv.DictReader(f):
            data[row["scheduler"]].append(
                (float(row["tte_secs"]), row["found"] == "true")
            )
    return data


def median(values):
    xs = sorted(values)
    n = len(xs)
    mid = n // 2
    if n % 2:
        return xs[mid]
    return (xs[mid - 1] + xs[mid]) / 2


def a12(a, b):
    """Vargha-Delaney A12: P(a is faster than b) + half the ties."""
    wins = 0
    ties = 0
    for x in a:
        for y in b:
            if x < y:
                wins += 1
            elif x == y:
                ties += 1
    return (wins + 0.5 * ties) / (len(a) * len(b))


def mann_whitney(a, b):
    """Two-sided Mann-Whitney U with a normal approximation.

    Returns (U, p) with average ranks so ties are handled.
    """
    pooled = [(v, 0) for v in a] + [(v, 1) for v in b]
    pooled.sort(key=lambda pair: pair[0])
    n = len(pooled)
    ranks = [0.0] * n
    i = 0
    while i < n:
        j = i
        while j + 1 < n and pooled[j + 1][0] == pooled[i][0]:
            j += 1
        average_rank = (i + j) / 2 + 1
        for k in range(i, j + 1):
            ranks[k] = average_rank
        i = j + 1

    n1 = len(a)
    n2 = len(b)
    rank_sum_a = sum(rank for rank, (_, group) in zip(ranks, pooled) if group == 0)
    u_a = rank_sum_a - n1 * (n1 + 1) / 2
    u = min(u_a, n1 * n2 - u_a)
    mu = n1 * n2 / 2
    sigma = math.sqrt(n1 * n2 * (n1 + n2 + 1) / 12)
    if sigma == 0:
        return u, 1.0
    z = (u - mu) / sigma
    return u, math.erfc(abs(z) / math.sqrt(2))


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else "bench_results.csv"
    data = load(path)
    baselines = [name for name in data if name != "directed"]
    directed = [tte for tte, _ in data.get("directed", [])]

    def row(name, samples, a12_value="-", p_value="-"):
        found = sum(1 for _, hit in data[name] if hit)
        return (
            f"| {name} | {found}/{len(data[name])} | {median(samples):.2f} "
            f"| {a12_value} | {p_value} |"
        )

    print("| Scheduler | Found | Median TTE (s) | A12 vs directed | Mann-Whitney p |")
    print("|---|---|---|---|---|")
    print(row("directed", directed))
    for baseline in baselines:
        samples = [tte for tte, _ in data[baseline]]
        _, p = mann_whitney(directed, samples)
        print(row(baseline, samples, f"{a12(directed, samples):.3f}", f"{p:.4f}"))


if __name__ == "__main__":
    main()
