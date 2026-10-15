"""Compares codecs from `cargo dev bench` CSV results (milestone M9).

    python scripts/bench_report.py bench-data/results/results-*.csv

Later files replace earlier rows for the same (image, format, setting), so a
partial rerun (e.g. `--format jpeg`) can be layered over a full run.

Prints, per image class:
- bits per pixel each codec needs to reach given SSIMULACRA2 targets, by
  interpolating its median quality curve (lower is better);
- the effort/method trade-offs at a fixed quality.
"""

import csv
import statistics
import sys
from collections import defaultdict

TARGETS = [70, 75, 80, 85, 90]
# The quality-curve settings of each lossy codec, from lowest to highest.
CURVES = {
    "jpg": ["q60", "q70", "q75", "q80", "q85", "q90", "q95"],
    "webp": ["q60 m4", "q70 m4", "q75 m4", "q80 m4", "q85 m4", "q90 m4", "q95 m4"],
    "jxl": ["d3 e7", "d2 e7", "d1.5 e7", "d1 e7", "d0.5 e7"],
    "avif": ["q40 e4", "q50 e4", "q60 e4", "q70 e4", "q80 e4"],
}
EFFORTS = {
    "jxl": ["d1 e4", "d1 e5", "d1 e6", "d1 e7", "d1 e9", "d1.5 e5", "d1.5 e6", "d1.5 e7", "lossless e5", "lossless e7"],
    "webp": ["q75 m4", "q75 m6", "q85 m4", "q85 m6"],
    "avif": ["q60 e2", "q60 e4", "q60 e6"],
    "png": ["c1", "c6", "c9"],
}


def load(paths):
    rows = {}
    for path in paths:
        with open(path, newline="", encoding="utf-8") as f:
            for row in csv.DictReader(f):
                if row["error"] or not row["ssimulacra2"]:
                    continue
                rows[(row["image"], row["format"], row["setting"])] = row
    return list(rows.values())


def stats(rows):
    """{(class, format, setting): (median score, p10 score, median bpp, median ms/MP)}"""
    groups = defaultdict(list)
    for row in rows:
        for cls in (row["class"], "all"):
            groups[(cls, row["format"], row["setting"])].append(row)
    out = {}
    for key, items in groups.items():
        scores = sorted(float(r["ssimulacra2"]) for r in items)
        bpp = statistics.median(float(r["bpp"]) for r in items)
        mpx = [int(r["width"]) * int(r["height"]) / 1e6 for r in items]
        speed = statistics.median(float(r["encode_ms"]) / m for r, m in zip(items, mpx))
        p10 = scores[round((len(scores) - 1) * 0.1)]
        out[key] = (statistics.median(scores), p10, bpp, speed)
    return out


def bpp_at(curve, target):
    """Linear interpolation of bpp at a target score along (score, bpp) points."""
    points = sorted(curve)
    for (s0, b0), (s1, b1) in zip(points, points[1:]):
        if s0 <= target <= s1 and s1 > s0:
            return b0 + (b1 - b0) * (target - s0) / (s1 - s0)
    return None


def main():
    rows = load(sys.argv[1:])
    table = stats(rows)
    classes = sorted({c for c, _, _ in table if c != "all"}) + ["all"]
    for cls in classes:
        print(f"\n## {cls}\n")
        print("bpp needed to reach a median SSIMULACRA2 of:\n")
        print("| codec | " + " | ".join(str(t) for t in TARGETS) + " |")
        print("|---|" + "---|" * len(TARGETS))
        for fmt, settings in CURVES.items():
            curve = [(table[(cls, fmt, s)][0], table[(cls, fmt, s)][2]) for s in settings if (cls, fmt, s) in table]
            cells = []
            for target in TARGETS:
                value = bpp_at(curve, target)
                cells.append(f"{value:.3f}" if value is not None else "–")
            print(f"| {fmt} | " + " | ".join(cells) + " |")
        print("\nSettings (median score, p10 score, median bpp, ms per megapixel):\n")
        for fmt, settings in list(CURVES.items()) + list(EFFORTS.items()):
            for setting in dict.fromkeys(settings):
                if (cls, fmt, setting) in table:
                    m, p10, bpp, speed = table[(cls, fmt, setting)]
                    print(f"  {fmt:5} {setting:12} {m:6.1f} {p10:6.1f} {bpp:7.3f} {speed:7.0f}")
        for fmt, setting in [("webp", "lossless m4"), ("jxl", "lossless e7")]:
            if (cls, fmt, setting) in table:
                m, p10, bpp, speed = table[(cls, fmt, setting)]
                print(f"  {fmt:5} {setting:12} {m:6.1f} {p10:6.1f} {bpp:7.3f} {speed:7.0f}")


if __name__ == "__main__":
    main()
