# Borescope Roadmap

## M0 — Git miner (standalone value, no tree-sitter)

- [ ] Workspace bootstrapped: `Cargo.toml` workspace, `bs-core` + `bs-git` crates, CI runs `cargo test`
- [ ] SQLite schema initialized: `files`, `git_stats`, `cochange`, `meta` tables; schema version in `meta`; delete+reindex produces same result
- [ ] Git log mined: `bs-git` shells out to `git log --numstat`; populates `files` and `git_stats` (churn, age per file); incremental by last-seen commit SHA
- [ ] Co-change matrix computed: `cochange` table populated; strength and support calculated; merge commits and mass-change commits excluded by default thresholds
- [ ] `borescope index --git` works end-to-end: exits 0, populates DB, adds `.borescope/` to `.gitignore`
- [ ] `borescope hotspots` renders ranked table; exit codes 0/1/4 correct
- [ ] `borescope coupled <file>` renders co-change partners above threshold
- [ ] `borescope age` renders file-level age view
- [ ] `borescope smells` detects and reports all four antipatterns (shotgun-surgery, god-file, stale-core, tangled-pair)
- [ ] `borescope map --weight churn --zoom pkg` renders containment tree at file/dir level
- [ ] Git miner tests pass: synthetic scripted history, deterministic churn/coupling numbers match expected

## M1 — Extraction

- [ ] `bs-extract` crate: tree-sitter runtime pinned; Go grammar compiled in; query pack captures `@def.function`, `@ref.call`, `@import`
- [ ] Symbol table populated from Go extraction; `symbols` and `edges` (kind=contains) tables added to schema
- [ ] Tier-1 grammars added (Rust, Python, TypeScript/TSX, JavaScript); extraction tests pass against fixture repos (precision ≥ 0.9 on confidence ≥ 0.7 edges)
- [ ] `borescope index` works on mixed-language repo including files with syntax errors; partial extraction, exit 0
- [ ] `borescope map --zoom fn` renders containment tree at symbol level

## M2 — Linking + slicing

- [ ] `bs-link`: import graph resolved per language; same-file call edges at confidence 1.0; cross-file edges confidence-scored per rubric
- [ ] `paths` command: forward slice from `file:Symbol`, `file:line`, or `QualifiedName`; exit 3 + JSON candidates on ambiguous target
- [ ] `callers` command: reverse slice; `--coupled` appends co-change section
- [ ] `tree` renderer: ANSI tree with weight bar, confidence annotation on dashed edges, depth-collapse marker
- [ ] `folded` renderer: Brendan Gregg format; `borescope paths X -o folded | inferno-flamegraph` produces valid SVG
- [ ] `json` renderer: schema 1 output; all required fields present; golden snapshot test passes
- [ ] Span-level git attribution: hotspot weight appears on individual frames; lazy blame cached in DB
- [ ] All weight modes work: `--weight none|loc|fanin|churn|hotspot`; normalized 0..1 within response

## M3 — Diff

- [ ] `diff [rev1 [rev2]]` command: frames classified `+/-/~`; context frames unmarked; defaults to HEAD vs worktree
- [ ] `branch <name>` command: sugar for diff from merge-base; correct merge-base detected
- [ ] `--weight diff` mode: frame weight = lines touched by diff
- [ ] HTML renderer: self-contained file, collapsible tree, weight bars, confidence styling, zoom toggle; no network requests; path printed to stdout
- [ ] Diff golden tests pass: fixture repo with two branches, `+/-/~` classification verified

## M4 — Distribution + skill

- [ ] Tier-2 grammars compiled in (Java, Ruby, C, C++); Tier 3 (Bash, HCL, YAML) parse + defs only
- [ ] `--grammar-path <dir>` dynamic loading with user-supplied query pack; documented pack format
- [ ] Release builds for linux-x64, linux-arm64, macos-arm64; binary ≤ 60 MB
- [ ] `skill/` package: `SKILL.md` protocol + `ensure-borescope.sh`; degrades gracefully when binary absent
- [ ] `docs/agent-contract.md` written; JSON schema 1 covered by golden tests
- [ ] `docs/eval.md`: harness + numbers showing skill agent reads fewer source bytes than unskilled agent on fixture task
- [ ] README with S1–S6 scenario recipes; all M0–M4 demos reproducible from scratch on clean machine
