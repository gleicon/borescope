#!/usr/bin/env python3
"""
Eval harness — measures source bytes read to complete the PaymentService rename task.

Usage:
    python harness/run_task.py --mode skill    [--borescope PATH] [--fixture PATH]
    python harness/run_task.py --mode baseline [--fixture PATH]

Outputs a JSON results file to results/<mode>_<timestamp>.json.
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

FIXTURE_REL = "testdata/eval-fixture"
GROUND_TRUTH_FILE = "ground_truth.json"
RESULTS_DIR = Path(__file__).parent.parent / "results"


def repo_root() -> Path:
    return Path(__file__).parent.parent


def fixture_path(override: str | None) -> Path:
    if override:
        return Path(override)
    return repo_root() / FIXTURE_REL


def find_borescope(override: str | None) -> Path:
    if override:
        return Path(override)
    # Check target/release first, then target/debug
    for candidate in [
        repo_root() / "target" / "release" / "borescope",
        repo_root() / "target" / "debug" / "borescope",
    ]:
        if candidate.exists():
            return candidate
    sys.exit("borescope binary not found; run `cargo build` first or pass --borescope PATH")


def run(cmd: list[str], cwd: Path = None) -> subprocess.CompletedProcess:
    return subprocess.run(
        cmd, capture_output=True, text=True, cwd=cwd or Path.cwd()
    )


def bytes_of(text: str) -> int:
    return len(text.encode())


def load_ground_truth(fixture: Path) -> dict:
    with open(fixture / GROUND_TRUTH_FILE) as f:
        return json.load(f)


# --------------------------------------------------------------------------- #
# Skill mode: use borescope callers to identify call sites                    #
# --------------------------------------------------------------------------- #

def run_skill_mode(borescope: Path, fixture: Path) -> dict:
    import tempfile

    gt = load_ground_truth(fixture)
    bytes_read = 0
    files_opened = []
    start = time.perf_counter()

    with tempfile.TemporaryDirectory() as tmp:
        # Copy fixture into hermetic temp dir
        tmp_fixture = Path(tmp) / "fixture"
        shutil.copytree(fixture, tmp_fixture)

        # Phase 1: index (no git — just structure)
        index_result = run(
            [str(borescope), "index", "--no-git", "--repo", str(tmp_fixture)],
            cwd=tmp_fixture,
        )
        if index_result.returncode != 0:
            sys.exit(f"borescope index failed:\n{index_result.stderr}")

        # Query callers of PaymentService.charge
        callers_result = run(
            [
                str(borescope),
                "-o", "json",
                "--depth", "3",
                "--repo", str(tmp_fixture),
                "callers", "payment_service.py:charge",
            ],
            cwd=tmp_fixture,
        )
        callers_json = callers_result.stdout
        bytes_read += bytes_of(callers_json)
        files_opened.append("borescope callers output (JSON)")

        # Parse which files have callers
        callers_data = json.loads(callers_json) if callers_result.returncode == 0 else {}
        caller_files = set()
        _collect_caller_files(callers_data.get("root"), caller_files)

        # Open only the caller files (simulated agent read)
        for rel_file in sorted(caller_files):
            abs_file = tmp_fixture / rel_file
            if abs_file.exists():
                content = abs_file.read_text()
                bytes_read += bytes_of(content)
                files_opened.append(rel_file)

        # Open payment_service.py to do the rename there too
        ps_file = tmp_fixture / "src" / "payment_service.py"
        if ps_file.exists() and "src/payment_service.py" not in caller_files:
            bytes_read += bytes_of(ps_file.read_text())
            files_opened.append("src/payment_service.py")

    elapsed = time.perf_counter() - start

    # Evaluate correctness: compare to all files that must be touched for the rename
    correct_callers = set(gt["rename_task"]["files_to_update"])
    found_callers = set(caller_files)
    precision = len(found_callers & correct_callers) / len(found_callers) if found_callers else 0
    recall = len(found_callers & correct_callers) / len(correct_callers) if correct_callers else 0
    missed = correct_callers - found_callers

    return {
        "mode": "skill",
        "bytes_read": bytes_read,
        "files_opened": files_opened,
        "files_opened_count": len(files_opened),
        "precision": round(precision, 3),
        "recall": round(recall, 3),
        "missed_callers": sorted(missed),
        "elapsed_s": round(elapsed, 3),
    }


def _collect_caller_files(node: dict | None, out: set) -> None:
    if node is None:
        return
    file = node.get("file", "")
    if file and not file.startswith("external:"):
        out.add(file)
    for child in node.get("children", []):
        _collect_caller_files(child, out)


# --------------------------------------------------------------------------- #
# Baseline mode: grep + read files manually                                   #
# --------------------------------------------------------------------------- #

def run_baseline_mode(fixture: Path) -> dict:
    import tempfile

    gt = load_ground_truth(fixture)
    bytes_read = 0
    files_opened = []
    start = time.perf_counter()

    with tempfile.TemporaryDirectory() as tmp:
        tmp_fixture = Path(tmp) / "fixture"
        shutil.copytree(fixture, tmp_fixture)

        # Simulate grep — agent reads all .py files looking for "charge"
        src_files = sorted((tmp_fixture / "src").glob("*.py"))
        for py_file in src_files:
            content = py_file.read_text()
            bytes_read += bytes_of(content)
            files_opened.append(str(py_file.relative_to(tmp_fixture)))

    elapsed = time.perf_counter() - start

    correct_callers = {c["file"] for c in gt["callers"]}
    # Baseline: agent reads all files so it finds all files to update (recall=1, precision varies)
    correct = set(gt["rename_task"]["files_to_update"])
    found = {f for f in files_opened if any(f.endswith(c.replace("src/", "")) for c in correct)}
    precision = len(found & correct) / len(found) if found else 0.0
    return {
        "mode": "baseline",
        "bytes_read": bytes_read,
        "files_opened": files_opened,
        "files_opened_count": len(files_opened),
        "precision": round(precision, 3),
        "recall": 1.0,
        "missed_callers": [],
        "elapsed_s": round(elapsed, 3),
    }


# --------------------------------------------------------------------------- #
# Main                                                                         #
# --------------------------------------------------------------------------- #

def main():
    parser = argparse.ArgumentParser(description="Borescope skill eval harness")
    parser.add_argument(
        "--mode",
        choices=["skill", "baseline"],
        required=True,
        help="skill = use borescope; baseline = read all files",
    )
    parser.add_argument("--borescope", help="Path to borescope binary")
    parser.add_argument("--fixture", help="Path to eval-fixture directory")
    parser.add_argument("--out", help="Output JSON path (default: results/<mode>_<ts>.json)")
    args = parser.parse_args()

    fixture = fixture_path(args.fixture)
    if not fixture.exists():
        sys.exit(f"Fixture not found: {fixture}")

    if args.mode == "skill":
        borescope = find_borescope(args.borescope)
        result = run_skill_mode(borescope, fixture)
    else:
        result = run_baseline_mode(fixture)

    result["fixture"] = str(fixture)
    result["timestamp"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())

    RESULTS_DIR.mkdir(parents=True, exist_ok=True)
    ts = int(time.time())
    out_path = Path(args.out) if args.out else RESULTS_DIR / f"{args.mode}_{ts}.json"
    out_path.write_text(json.dumps(result, indent=2))

    print(json.dumps(result, indent=2))
    print(f"\nSaved to: {out_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
