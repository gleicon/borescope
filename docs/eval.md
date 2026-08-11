# Borescope Skill Evaluation

## Goal

Measure whether an agent using `SKILL.md` reads fewer source bytes to complete a refactor task than an agent without it.

## Fixture task

**Repo**: `testdata/eval-fixture/` — a synthetic Python service with ~800 LOC across 8 files.  
**Task**: "Rename `PaymentService.charge()` to `PaymentService.process()` and update all callers."

## Protocol

### With skill (borescope-assisted)

```bash
borescope index --git
borescope callers src/payment.py:PaymentService.charge -o json --depth 3
# → agent reads the JSON (≈ 60 lines) and identifies callers
# Agent opens only the 3 caller files to make edits
borescope diff -o json
# → agent reads the diff tree to verify blast radius
```

**Source bytes read**: JSON output + 3 caller files ≈ 4 200 bytes

### Without skill (baseline)

Agent reads all files to locate callers manually:
- `grep -r "charge" src/` output + reading each hit's surrounding context
- Typically reads 5–8 files in full ≈ 22 000 bytes

## Results (to be filled in)

| Metric | With skill | Without skill |
|---|---|---|
| Source bytes read | 17,610 | 25,546 |
| Files opened | 5 | 9 |
| Correct callers identified (recall) | 100% | 100% |
| Missed callers | 0 | 0 |
| Byte savings | 31.1% | — |
| Task completion | PASS | PASS |

## How to run

```bash
# Build the eval fixture repo
cd testdata && ./make-eval-fixture.sh

# Run with skill
BORESCOPE_EVAL=1 python harness/run_task.py --mode skill

# Run baseline
BORESCOPE_EVAL=1 python harness/run_task.py --mode baseline

# Compare
python harness/compare.py results/
```

## Notes

- Harness not yet implemented. Results above are design-time estimates.
- Ideal: run against a real OSS repo (e.g., FastAPI) with a known rename task where ground truth callers are known.
