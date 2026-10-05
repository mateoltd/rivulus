#!/usr/bin/env python3
"""Tolerance-band perf compare (warn, never fail).

Compares a current bench JSON artifact against a baseline artifact.
Breaches print WARN but the exit code stays 0: CI graphs the numbers and
alerts on bands instead of gating on absolute RSS (plans/05).

Usage:
    bench/compare.py --baseline bench/results/replay-2026-09-27.json \
                     --current bench/results/replay-local.json
    bench/compare.py --baseline ... --current ... --tolerance-pct 20

Checks (per case, current vs baseline):
    allocs_per_event, alloc_bytes_per_event, p99_ms
For `rest_ratelimit` artifacts: available.p99_ms, preemptive.wait_ms (lower
bound: current must stay >= 50% of baseline wait, else the wait regressed).
"""

import argparse
import json
import sys

METRICS = ("allocs_per_event", "alloc_bytes_per_event", "p99_ms")


def load(path):
    with open(path) as f:
        return json.load(f)


def check_ratio(name, metric, base, cur, tol):
    if base == 0:
        ok = cur == 0
        ratio = 0.0 if ok else float("inf")
    else:
        ratio = cur / base
        ok = ratio <= 1.0 + tol
    status = "OK  " if ok else "WARN"
    print(
        f"{status} {name} {metric}: baseline={base:.4g} current={cur:.4g} "
        f"ratio={ratio:.3f} (band +{tol * 100:.0f}%)"
    )
    return ok


def main():
    ap = argparse.ArgumentParser(description="tolerance-band bench compare (warn-only)")
    ap.add_argument("--baseline", required=True)
    ap.add_argument("--current", required=True)
    ap.add_argument("--tolerance-pct", type=float, default=20.0)
    args = ap.parse_args()
    tol = args.tolerance_pct / 100.0

    try:
        base = load(args.baseline)
        cur = load(args.current)
    except (OSError, json.JSONDecodeError) as e:
        print(f"ERROR: cannot read artifacts: {e}")
        return 2

    ok_all = True
    if base.get("bench") != cur.get("bench"):
        print(
            f"ERROR: bench mismatch ({base.get('bench')} vs {cur.get('bench')}); "
            "compare like-for-like artifacts only."
        )
        return 2

    if base.get("bench") == "replay":
        b_cases = {c["name"]: c for c in base["cases"]}
        for c in cur["cases"]:
            b = b_cases.get(c["name"])
            if b is None:
                print(f"WARN {c['name']}: no baseline case, skipping")
                ok_all = False
                continue
            for m in METRICS:
                ok_all &= check_ratio(c["name"], m, b[m], c[m], tol)
    elif base.get("bench") == "rest_ratelimit":
        ok_all &= check_ratio(
            "available", "p99_ms",
            base["available"]["p99_ms"], cur["available"]["p99_ms"], tol,
        )
        # Pre-emptive wait must not shrink away: warn if it collapses.
        waited = cur["preemptive"]["wait_ms"]
        expected = base["preemptive"]["reset_after_ms"]
        kept = waited >= expected * 0.5
        print(
            f"{'OK  ' if kept else 'WARN'} preemptive wait_ms: "
            f"current={waited:.1f} reset_after={expected:.1f}"
        )
        ok_all &= kept
        for key in ("shared_excluded",):
            same = cur[key] == base[key]
            print(f"{'OK  ' if same else 'WARN'} {key}: {cur[key]}")
            ok_all &= same
    else:
        print(f"ERROR: unknown bench {base.get('bench')!r}")
        return 2

    print("COMPARE PASS (within bands)" if ok_all else "COMPARE WARN (band breach — investigate, do not gate)")
    return 0  # warn-only: never fail the build on perf drift


if __name__ == "__main__":
    sys.exit(main())
