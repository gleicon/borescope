# Borescope

**Call graphs from structure, not execution. In every language, without running the code.**

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

Index any repo in seconds. Ask "what calls this?", "what does this reach?", "what changed?", "what's risky?" — and get answers you can act on, not just grep output.

---

## Install

```bash
cargo install --path crates/bs-cli   # from source, requires Rust 1.70+
# or: download a release binary from GitHub Releases
```

## 30-second start

```bash
cd your-repo

# Two-phase index: structural queries work immediately; git signals load in background
borescope index --no-git            # fast — paths/callers/map/explain ready now
borescope index --git &             # background — hotspots/smells/age ready when done

borescope hotspots                  # what's hot and complex?
borescope map --weight hotspot -o tui   # navigate the whole repo interactively
borescope smells                    # antipattern + semantic risk report
borescope explain src/auth.rs:verify    # plain-English symbol profile
```

---

## Why borescope

### You're about to edit a function you didn't write
```bash
borescope callers src/auth/token.go:Verify --depth 4 -o tui
```
See everything that depends on it before you touch a line. The bar chart is churn × recency — read it as blast radius × fire risk.

### You're reviewing a PR and don't trust the diff alone
```bash
borescope explain-pr feature/payments
borescope diff main HEAD --weight hotspot
```
Flags high-risk symbols, co-change partners missing from the PR, and concurrency patterns in touched code. Bottom-line risk verdict at the end.

### You inherited a codebase and have no map
```bash
borescope index --git
borescope smells
borescope hotspots --top 20
borescope map --weight hotspot -o tui
```
In under a minute: which files are tightly coupled, which symbols are hot and complex, which have dangerous concurrency patterns, and a navigable call graph of the whole thing.

### You're using an agent to do the work
```bash
borescope callers src/payment.py:charge -o json --depth 3
borescope paths src/payment.py:charge --analyze
borescope explain-pr feature/refactor -o json
```
Cuts source bytes read by ~31% on rename tasks vs grep-and-read-all-files. Stable JSON contract (schema 1) — see [`docs/agent-contract.md`](docs/agent-contract.md).

---

## Languages

**Tier 1** — full extraction, linking, semantic patterns: Go · Rust · Python · TypeScript/TSX · JavaScript  
**Tier 2** — extraction, linking, patterns: Java · Ruby · C · C++  
**Tier 3** — definitions only: Bash  
**Custom** — `--grammar-path <dir>` with a `.so` + `.scm` query pack

---

## Go deeper

| Topic | File |
|---|---|
| All commands + flags + exit codes | [`docs/commands.md`](docs/commands.md) |
| Output formats (TUI, JSON, folded, HTML) | [`docs/output-formats.md`](docs/output-formats.md) |
| Semantic patterns + custom smell rules | [`docs/patterns.md`](docs/patterns.md) |
| Cookbook workflows (human + agent) | [`RECIPES.md`](RECIPES.md) |
| JSON agent contract (schema 1) | [`docs/agent-contract.md`](docs/agent-contract.md) |
| Agent skill protocol | [`skill/SKILL.md`](skill/SKILL.md) |
| Performance targets + storage | [`docs/performance.md`](docs/performance.md) |
| Full product + technical spec | [`docs/SPEC.md`](docs/SPEC.md) |
| Skill eval methodology + results | [`docs/eval.md`](docs/eval.md) |

---

## Build

```bash
cargo build --release
cargo test
cargo install --path crates/bs-cli --force
```

`.borescope/index.db` is SQLite, auto-added to `.gitignore`. Delete and re-run `index` to rebuild from scratch at any time.
