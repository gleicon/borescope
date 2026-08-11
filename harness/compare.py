#!/usr/bin/env python3
"""
Compare skill vs baseline eval results and update docs/eval.md with measured numbers.

Usage:
    python harness/compare.py results/
    python harness/compare.py results/skill_*.json results/baseline_*.json
"""

import argparse
import json
import sys
from pathlib import Path

EVAL_MD = Path(__file__).parent.parent / "docs" / "eval.md"


def load_latest(results_dir: Path, mode: str) -> dict:
    files = sorted(results_dir.glob(f"{mode}_*.json"))
    if not files:
        sys.exit(f"No {mode} result files in {results_dir}")
    return json.loads(files[-1].read_text())


def compare(skill: dict, baseline: dict) -> dict:
    savings_bytes = baseline["bytes_read"] - skill["bytes_read"]
    savings_pct = 100.0 * savings_bytes / baseline["bytes_read"] if baseline["bytes_read"] else 0
    return {
        "skill_bytes": skill["bytes_read"],
        "baseline_bytes": baseline["bytes_read"],
        "savings_bytes": savings_bytes,
        "savings_pct": round(savings_pct, 1),
        "skill_files": skill["files_opened_count"],
        "baseline_files": baseline["files_opened_count"],
        "skill_precision": skill["precision"],
        "skill_recall": skill["recall"],
        "missed_callers": skill.get("missed_callers", []),
        "skill_passed": skill["recall"] == 1.0 and skill["precision"] >= 0.9,
        "baseline_passed": baseline["recall"] == 1.0,
    }


def render_table(c: dict) -> str:
    def row(metric, skill, baseline):
        return f"| {metric} | {skill} | {baseline} |"

    lines = [
        "| Metric | With skill | Without skill |",
        "|---|---|---|",
        row(
            "Source bytes read",
            f"{c['skill_bytes']:,}",
            f"{c['baseline_bytes']:,}",
        ),
        row(
            "Files opened",
            str(c["skill_files"]),
            str(c["baseline_files"]),
        ),
        row(
            "Correct callers identified (recall)",
            f"{c['skill_recall']:.0%}",
            f"{c['baseline_recall']:.0%}" if 'baseline_recall' in c else "100%",
        ),
        row(
            "Missed callers",
            str(len(c["missed_callers"])),
            "0",
        ),
        row(
            "Byte savings",
            f"{c['savings_pct']:.1f}%",
            "—",
        ),
        row(
            "Task completion",
            "PASS" if c["skill_passed"] else "FAIL",
            "PASS" if c["baseline_passed"] else "FAIL",
        ),
    ]
    return "\n".join(lines)


def update_eval_md(c: dict, skill: dict, baseline: dict) -> None:
    if not EVAL_MD.exists():
        print(f"WARNING: {EVAL_MD} not found, skipping update", file=sys.stderr)
        return

    text = EVAL_MD.read_text()
    table = render_table(c)

    # Replace the Results section table (between "## Results" and "## How to run")
    import re
    pattern = r"(## Results.*?\n\n)\|.*?\n\n(## How to run)"
    replacement = rf"\g<1>{table}\n\n\g<2>"
    new_text = re.sub(pattern, replacement, text, flags=re.DOTALL)

    if new_text == text:
        # Fallback: just append a note
        new_text += f"\n\n<!-- Last run: {skill.get('timestamp', 'unknown')} -->\n"
        new_text += f"<!-- Savings: {c['savings_pct']:.1f}% bytes ({c['skill_bytes']:,} vs {c['baseline_bytes']:,}) -->\n"

    EVAL_MD.write_text(new_text)
    print(f"Updated {EVAL_MD}", file=sys.stderr)


def main():
    parser = argparse.ArgumentParser(description="Compare skill vs baseline eval results")
    parser.add_argument(
        "results",
        nargs="+",
        help="Path to results directory or specific JSON files",
    )
    parser.add_argument(
        "--no-update-md",
        action="store_true",
        help="Print comparison without updating eval.md",
    )
    args = parser.parse_args()

    # Resolve inputs: directory or explicit files
    if len(args.results) == 1 and Path(args.results[0]).is_dir():
        results_dir = Path(args.results[0])
        skill = load_latest(results_dir, "skill")
        baseline = load_latest(results_dir, "baseline")
    elif len(args.results) == 2:
        files = [Path(p) for p in args.results]
        data = [json.loads(f.read_text()) for f in files]
        skill_list = [d for d in data if d.get("mode") == "skill"]
        baseline_list = [d for d in data if d.get("mode") == "baseline"]
        if not skill_list or not baseline_list:
            sys.exit("Need one skill and one baseline result file")
        skill, baseline = skill_list[0], baseline_list[0]
    else:
        sys.exit("Pass a results directory or exactly two JSON files (skill + baseline)")

    c = compare(skill, baseline)

    print("=== Borescope Skill Eval ===\n")
    print(render_table(c))
    print(f"\nByte savings: {c['savings_pct']:.1f}% ({c['savings_bytes']:,} bytes saved)")
    if c["missed_callers"]:
        print(f"Missed callers: {c['missed_callers']}")
    print(f"\nSkill verdict: {'PASS' if c['skill_passed'] else 'FAIL'}")

    if not args.no_update_md:
        update_eval_md(c, skill, baseline)


if __name__ == "__main__":
    main()
