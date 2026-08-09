# Borescope Recipes

Cookbook of common workflows — human and agent.

## Quick reference

```bash
borescope index --git                        # always run first; safe to re-run
borescope map --weight hotspot -o tui        # interactive explorer
borescope smells                             # grouped antipattern + semantic report
borescope explain <symbol>                   # plain-English profile
borescope explain-pr <branch>                # PR impact analysis
```

---

## 1. Explore an unfamiliar codebase

```bash
cd some-repo
borescope index --git
borescope smells                         # where are the structural + semantic problems?
borescope hotspots --top 15              # what's actively risky?
borescope map --weight hotspot -o tui    # navigate the whole tree interactively
borescope explain <top-hotspot-fn>       # understand the riskiest symbol
```

**What you get**: within 30 seconds you know which symbols are hot/complex/central, which files are tightly coupled, which have dangerous concurrency patterns, and a plain-English profile of the worst offender.

---

## 2. PR review — understand impact before merging

```bash
# Narrative PR impact report
borescope explain-pr feature/my-branch
borescope explain-pr feature/my-branch --base develop   # explicit base

# Call-tree diff view
borescope diff main HEAD --weight hotspot
borescope branch feature/my-branch -o tui               # interactive branch diff
```

`explain-pr` reports:
- Changed files (capped at 20)
- High-risk symbols (hot + complex, or dangerous patterns)
- Co-change warnings: files that usually move with these but aren't in the PR
- Semantic patterns in touched code (lock, await, spawn…)
- Bottom-line risk verdict

`-o json` gives the full list for tooling:
```bash
borescope explain-pr feature/my-branch -o json | jq '.high_risk[] | .qualified'
```

---

## 3. Blast radius before touching a function

```bash
borescope callers src/worker/pool.rs:dispatch --depth 4 -o tui
borescope callers src/worker/pool.rs:dispatch -o json   # for agents
```

Shows everything that calls your target, recursively. Low-confidence edges (┄) are cross-file guesses — treat them as leads.

---

## 4. Plain-English symbol profile

```bash
borescope explain dispatch_to_worker_pool
borescope explain src/http/router.rs:handle
borescope explain src/http/router.rs:42      # by line number
```

Output:
- File, kind, LOC, complexity
- Hotspot rating with narrative (cold / lukewarm / warm / 🔥 very hot)
- Fanin/fanout with blast-radius interpretation
- Semantic pattern warnings (⚠ lock+await, ⚠ block_on)
- Co-change partners with coupling strength
- Risk verdict (LOW / MEDIUM / HIGH RISK)

---

## 5. Forward exploration from an entry point

```bash
borescope paths src/http/router.rs:dispatch_to_worker_pool --depth 5 -o tui
borescope paths src/http/router.rs:dispatch_to_worker_pool --weight churn
```

The TUI detail panel (cyan bar) shows the weight description and selected node's score + file. The confidence tag `┄0.3` means the linker guessed the edge — not a guaranteed call.

---

## 6. Find what always changes together

```bash
borescope coupled src/worker/pool.rs
borescope coupled src/runtime/apis.rs --min 0.5   # only strong pairs
```

Co-change strength ≥ 0.8 both ways = tangled pair. Candidate for interface refactor or merge. Invisible in the call graph, visible immediately in git history.

---

## 7. Semantic antipattern audit

```bash
borescope smells
borescope smells --recommend    # adds cargo audit / semgrep suggestions
```

Findings are grouped by kind with a description and top 3 examples:

```
[lock_across_await] — mutex held across .await — deadlock risk (148 symbols)
  • src/worker/pool.rs:dispatch_to_worker_pool
  • src/runtime/handler.rs:handle_request
  … and 146 more
```

`--recommend` checks co-change partners for security-sensitive filenames (auth, crypto, token, secret…) and suggests `cargo audit` or `semgrep --config=p/security-audit`.

---

## 8. Identify technical debt before a sprint

```bash
borescope smells
borescope hotspots --top 20
borescope age
```

`smells` reports:
- **shotgun-surgery**: one file with ≥4 strong co-change partners → changes ripple everywhere
- **god-file**: high LOC + high complexity + high fanin
- **stale-core**: rarely touched but many callers depend on it
- **tangled-pair**: two files that always change together → missing abstraction
- **Semantic**: lock_across_await, sync_in_async, alloc_in_hotspot, high_complexity_bottleneck, spawn_in_tight_loop, unbalanced_fanout

---

## 9. Agent context preparation (skill workflow)

Before reading source files, ask borescope for the relevant slice:

```bash
# Agent receives: "refactor PaymentService.charge"
borescope index --git
borescope explain src/payment.py:charge          # understand the symbol first
borescope callers src/payment.py:charge -o json --depth 3
# → agent reads JSON, identifies caller files, opens only those
borescope explain-pr feature/refactor -o json   # verify PR impact before merge
```

Typically cuts source bytes read by 70–85% vs grep-and-read-all-files.

---

## 10. Post-security-patch impact check

```bash
borescope diff <before-sha> <after-sha> --weight hotspot
borescope coupled src/runtime/fetch.rs
borescope smells --recommend
```

`coupled` reveals which other files are historically co-changed with the patched file. `smells --recommend` checks if co-change partners have security-sensitive names and suggests external scanners.

---

## 11. JSON output for scripting

All commands support `-o json`. Schema 1 is stable (additive only):

```bash
borescope hotspots --top 5 -o json | jq '.nodes[] | {name: .name, hotspot: .weight}'
borescope callers src/auth.rs:verify -o json | jq '.root.children | length'
borescope smells -o json | jq '.semantic | group_by(.kind) | map({kind: .[0].kind, count: length})'
borescope explain dispatch_to_worker_pool -o json | jq '{hotspot, complexity, fanin, patterns}'
borescope explain-pr feature/branch -o json | jq '.missed_cochange'
```

---

## 12. Generate a flamegraph from the call tree

```bash
borescope paths src/http/router.rs:dispatch_to_worker_pool -o folded | inferno-flamegraph > flame.svg
open flame.svg
```

Folded output is Brendan Gregg format. Install `inferno`: `cargo install inferno`.

---

## TUI keybindings

| Key | Action |
|---|---|
| `j` / `↓` | move down |
| `k` / `↑` | move up |
| `Enter` / `Space` | expand / collapse node |
| `g` | jump to top |
| `G` | jump to bottom |
| `/` | enter filter mode (by name or file) |
| `Esc` | exit filter mode / quit |
| `q` | quit |

The cyan detail panel above the help line shows the weight description for the current view and the selected node's exact score + file path.
