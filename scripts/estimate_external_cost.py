#!/usr/bin/env python3
"""Estimate what an externally produced run would have cost.

A model driven outside the Rust client records no token usage, so `vcr
evaluate` reports its cost as unavailable rather than as zero. This produces a
clearly-labelled ESTIMATE instead, anchored to two real quantities:

  * the input text is byte-identical to what the measured arm received, so that
    arm's measured input-token count applies to it directly;
  * the output text exists on disk, so its token count can be estimated at the
    chars-per-token ratio observed on the input.

The result is an ESTIMATE, not a measurement. Tokenizers differ between model
families, so treat it as good to roughly +/-15% in an unknown direction. It
belongs in prose that says so, and never in a results table.

    python scripts/estimate_external_cost.py \\
        --prompts /tmp/sonnet-prompts \\
        --responses /tmp/sonnet-responses \\
        --measured results/evaluation-baseline.json \\
        --price-in 2.00 --price-out 10.00 --label "Sonnet 5 standard"
"""

import argparse
import glob
import io
import json
import os
import sys


def total_chars(paths):
    return sum(len(io.open(p, encoding="utf-8").read()) for p in paths)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--prompts", required=True, help="directory from export_baseline_prompts.py")
    ap.add_argument("--responses", required=True, help="directory of the external model's answers")
    ap.add_argument(
        "--measured",
        default="results/evaluation-baseline.json",
        help="evaluation of the arm whose tokens WERE measured on identical input",
    )
    ap.add_argument("--price-in", type=float, required=True, help="USD per million input tokens")
    ap.add_argument("--price-out", type=float, required=True, help="USD per million output tokens")
    ap.add_argument("--label", default="external model")
    args = ap.parse_args()

    sys_files = sorted(glob.glob(os.path.join(args.prompts, "*.system.txt")))
    usr_files = sorted(glob.glob(os.path.join(args.prompts, "*.user.txt")))
    res_files = sorted(glob.glob(os.path.join(args.responses, "*.json")))

    if not sys_files or not res_files:
        print("no prompt or response files found; check --prompts/--responses", file=sys.stderr)
        return 2

    in_chars = total_chars(sys_files + usr_files)
    out_chars = total_chars(res_files)

    measured = json.loads(io.open(args.measured, encoding="utf-8").read())["aggregate"]
    measured_in = measured["total_input_tokens"]
    measured_out = measured["total_output_tokens"]
    cases = measured["case_count"]

    if measured_in == 0:
        print(f"{args.measured} has no measured tokens to anchor against", file=sys.stderr)
        return 2

    ratio = in_chars / measured_in
    est_out = out_chars / ratio

    print(f"cases                       : {cases}")
    print(f"input text (identical bytes): {in_chars:,} chars")
    print(f"input tokens                : {measured_in:,}  [MEASURED on this exact text]")
    print(f"observed ratio              : {ratio:.2f} chars/token")
    print()
    print(f"{args.label} output text  : {out_chars:,} chars")
    print(f"{args.label} output tokens: {est_out:,.0f}  [ESTIMATE]")
    print(f"reference output tokens     : {measured_out:,}  [MEASURED, other model]")
    print()

    est = measured_in / 1e6 * args.price_in + est_out / 1e6 * args.price_out
    print(f"{args.label}: ${est:.4f} total -> ${est / cases:.5f}/case   [ESTIMATE]")
    print()
    print("This is an estimate. Tokenizers differ between model families, the input")
    print("token count is borrowed from the measured arm, and driving a model through")
    print("an agent harness consumes more than the equivalent direct API call would.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
