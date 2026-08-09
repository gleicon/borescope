# Borescope

Static call-path engine. Gives you flamegraphs from structure, not execution — in every language, without running the code.

```
borescope paths api/checkout.go:HandleCheckout --depth 4 --weight hotspot
```

```
HandleCheckout                           ██████ 0.82
├─ validateCart()                        ████   0.61
│  ├─ PriceService.quote()               ▌      0.07
│  └─ Inventory.check()                  █      0.12
├─ services.boot()                       ▌      0.05
└─ SessionManager.open()   ┄┄ 0.5        █      0.15
```

## What it does

- Parses any repo into a symbol/call graph via tree-sitter — zero config, no build step
- Mines git history for churn, code age, hotspot score, and co-change coupling
- Detects semantic patterns (locks, awaits, spawns, allocations) and cross-signals them with git signals to surface real risk
- Answers call-path queries as weighted, confidence-annotated trees
- Explains symbols and PRs in plain English
- Exports to ANSI tree, interactive TUI, Brendan Gregg folded (pipeable to `inferno`), JSON (stable agent contract), and self-contained HTML

## Install

```bash
# From source (requires Rust 1.70+)
cargo install --path crates/bs-cli

# Or via make
make ci-install   # uses --force, safe to re-run

# Or: download a release binary from GitHub Releases
```

## Quick start

```bash
cd your-repo
borescope index --git           # build index + mine git history (~30s for 300k LOC)
borescope hotspots              # see what's risky
borescope map --weight hotspot -o tui   # interactive TUI explorer
borescope smells                # antipattern + semantic report
borescope explain dispatch_to_worker_pool   # plain-English symbol profile
borescope explain-pr my-branch  # PR impact analysis
```

## Scenarios

### S1 — PR review

```bash
# Narrative impact report
borescope explain-pr feature/my-branch
borescope explain-pr feature/my-branch --base develop

# Call-tree diff
borescope diff main HEAD --weight hotspot
```

Explains which files changed, which symbols are high-risk (hot + complex + central), which co-change partners are missing from the PR, semantic patterns in touched code, and a bottom-line verdict.

### S2 — Entry-point exploration

```bash
borescope paths api/checkout.go:HandleCheckout --depth 4 -o tui
```

Interactive tree of everything statically reachable from an HTTP handler. The status bar shows what the score means. Navigate with `j`/`k`, expand/collapse with Enter, filter with `/`.

### S3 — Blast radius before editing

```bash
borescope callers internal/auth/token.go:Verify -o tui
borescope callers internal/auth/token.go:Verify -o json   # for agents
```

Reverse slice. The TUI detail panel shows hotspot + file for the selected node.

### S4 — Symbol profile

```bash
borescope explain dispatch_to_worker_pool
borescope explain src/http/router.rs:handle
```

Plain-English narrative: heat rating, complexity, fanin/fanout, concurrency pattern warnings, co-change partners, risk verdict.

### S5 — Repo archaeology

```bash
borescope smells
borescope smells --recommend     # adds cargo audit / semgrep suggestions
borescope coupled src/billing/invoice.py --min 0.5
borescope age --zoom pkg
```

`smells` groups findings by kind with description and top examples. `--recommend` checks co-change partners for security-sensitive filenames and suggests external tools.

### S6 — Branch visualization

```bash
borescope branch feature/new-pricing -o tui
```

Impact tree for an entire branch vs its merge-base.

## Commands

| Command | Description |
|---|---|
| `index [--full] [--git]` | Build/update `.borescope/` |
| `paths <target>` | Forward slice — everything reachable from target |
| `callers <target>` | Reverse slice — all callers of target |
| `explain <target>` | Plain-English symbol profile with risk verdict |
| `explain-pr <branch> [--base main]` | PR impact: risk, blast radius, co-change warnings |
| `diff [rev1 [rev2]]` | Call-tree diff |
| `branch <name> [--base rev]` | Branch impact tree |
| `map` | Repo overview |
| `hotspots [--top N]` | Churn × complexity ranking |
| `coupled <file> [--min F]` | Co-change partners |
| `age` | Code-age view |
| `smells [--recommend]` | Antipattern + semantic pattern report |

## Global flags

```
--depth N           max tree depth (default: 3)
--zoom pkg|mod|fn   aggregation level (default: fn)
--weight none|loc|fanin|churn|hotspot|diff
-o tree|folded|json|html|tui
--min-confidence F  hide edges below threshold
--no-color          plain ASCII
```

## Output formats

| Format | Description |
|---|---|
| `tree` (default) | ANSI call tree with weight bars and confidence annotations |
| `tui` | Interactive terminal UI — navigate, expand/collapse, filter, score explained |
| `folded` | Brendan Gregg format — pipe to `inferno-flamegraph` |
| `json` | Stable schema 1 contract for agent consumption |
| `html` | Self-contained collapsible tree, no network requests |

### TUI

The TUI has a persistent detail panel (cyan bar above the help line) showing:
- What the current weight mode means (e.g., `score: hotspot  (churn × recency, 0=cold 1=hot)`)
- The selected node's exact score and file path

| Key | Action |
|---|---|
| `j` / `↓` | move down |
| `k` / `↑` | move up |
| `Enter` / `Space` | expand / collapse |
| `g` / `G` | jump to top / bottom |
| `/` | filter by name or file |
| `Esc` / `q` | exit |

## Semantic pattern detection

During indexing, tree-sitter captures structural patterns in every symbol:

| Pattern | What it means |
|---|---|
| `lock` | Mutex/RwLock acquisition |
| `await` | Async yield point |
| `block_on` | Blocking inside async context |
| `spawn` | Thread/goroutine/task creation |
| `loop` | Any loop construct |
| `alloc` | Allocating calls (clone, collect, new…) |
| `chan` | Channel send/receive |
| `timer` | setTimeout / ticker |

`smells` cross-signals these with git signals:

| Detector | Signal combination |
|---|---|
| `lock_across_await` | lock + await in same fn — deadlock risk |
| `sync_in_async` | block_on in async fn — starves executor |
| `alloc_in_hotspot` | many allocs + hotspot > 0.7 |
| `high_complexity_bottleneck` | complexity > 15 + fanin > 10 + hotspot > 0.6 |
| `spawn_in_tight_loop` | spawn + loop — goroutine/thread explosion |
| `unbalanced_fanout` | fanout > 8 + fanin < 2 + low churn — likely dead |

## Target syntax

```
path/to/file.go:FuncName      # file + name
path/to/file.go:42            # file + line number (resolves to enclosing symbol)
QualifiedName                 # global name search (exit 3 if ambiguous; lists candidates)
```

## Supported languages

**Tier 1** (full extraction + linking + patterns): Go, Rust, Python, TypeScript/TSX, JavaScript
**Tier 2** (extraction + linking + patterns): Java, Ruby, C, C++
**Tier 3** (parse + defs only): Bash

Custom grammars: `--grammar-path <dir>` with `<lang>.so` and `<lang>.scm` query pack.

## Exit codes

```
0  success
1  runtime error
2  usage error
3  ambiguous target (candidates on stderr as JSON with kinds)
4  index missing — run `borescope index`
5  grammar unavailable
```

## For coding agents

See `skill/SKILL.md` for the agent protocol and `RECIPES.md` for cookbook workflows.

```bash
borescope index --git
borescope explain-pr feature/branch -o json   # PR impact as JSON
borescope callers src/payment.py:charge -o json --depth 3
borescope explain src/payment.py:charge       # narrative profile
```

## Storage

`.borescope/index.db` — SQLite, auto-added to `.gitignore`. Delete it anytime; re-running any command rebuilds it. The `patterns` column is added automatically on first index after upgrading — no manual migration needed.

## Build

```bash
make build        # debug
make release      # optimized
make test
make ci-install   # cargo install --path crates/bs-cli --force
make tag          # git tag v<version> + push (triggers release CI)
make dev-smells   # run smells without installing (cargo run)
```

## Performance targets (release build, 8-core)

| Operation | Target |
|---|---|
| Cold index, 300k LOC | < 30 s |
| +git history, 10k commits | + < 20 s |
| Incremental, 10 changed files | < 1 s |
| `paths`/`callers` depth 4 | < 200 ms |
| `diff` on 50-file PR | < 2 s |
| Binary size (all grammars) | < 60 MB |
