# Borescope Roadmap

## M0 — Git miner (standalone value, no tree-sitter)

- [x] Workspace bootstrapped: `Cargo.toml` workspace, `bs-core` + `bs-git` crates, CI runs `cargo test`
- [x] SQLite schema initialized: `files`, `git_stats`, `cochange`, `meta` tables; schema version in `meta`; delete+reindex produces same result
- [x] Git log mined: `bs-git` shells out to `git log --numstat`; populates `files` and `git_stats` (churn, age per file); incremental by last-seen commit SHA
- [x] Co-change matrix computed: `cochange` table populated; strength and support calculated; merge commits excluded by default
- [x] `borescope index --git` works end-to-end: exits 0, populates DB, adds `.borescope/` to `.gitignore`
- [x] `borescope hotspots` renders ranked table; exit codes 0/1/4 correct
- [x] `borescope coupled <file>` renders co-change partners above threshold
- [x] `borescope age` renders file-level age view
- [x] `borescope smells` detects and reports all four antipatterns (shotgun-surgery, god-file, stale-core, tangled-pair)
- [x] `borescope map --weight churn --zoom pkg` renders containment tree at file/dir level
- [x] Git miner tests pass: synthetic scripted history, deterministic churn/coupling numbers match expected (4 tests)

## M1 — Extraction

- [x] `bs-extract` crate: tree-sitter 0.26 runtime pinned; Go grammar compiled in; query pack captures `@def.function`, `@ref.call`, `@import`
- [x] Symbol table populated from Go extraction; `symbols` and `edges` (kind=contains) tables added to schema
- [x] Tier-1 grammars added (Rust, Python, TypeScript/TSX, JavaScript); extraction tests pass against fixture repos
- [x] `borescope index` works on mixed-language repo including files with syntax errors; partial extraction, exit 0
- [x] `borescope map --zoom fn` renders containment tree at symbol level (symbols grouped by file)

## M2 — Linking + slicing

- [x] `bs-link`: import graph resolved per language; same-file call edges at confidence 1.0; cross-file edges confidence-scored per rubric
- [x] `paths` command: forward slice from `file:Symbol`, `file:line`, or `QualifiedName`; exit 3 + JSON candidates on ambiguous target
- [x] `callers` command: reverse slice; `--coupled` appends co-change section
- [x] `tree` renderer: ANSI tree with weight bar, confidence annotation on dashed edges, depth-collapse marker
- [x] `folded` renderer: Brendan Gregg format; pipeable to `inferno-flamegraph`
- [x] `json` renderer: schema 1 output; all required fields present
- [x] Span-level git attribution: hunk-overlap approach; hotspot weight computed per symbol span; cached in DB
- [x] All weight modes work: `--weight none|loc|fanin|churn|hotspot|diff`; normalized 0..1 within response

## M3 — Diff

- [x] `diff [rev1 [rev2]]` command: frames classified `+/-/~`; defaults to HEAD vs worktree
- [x] `branch <name>` command: sugar for diff from merge-base; correct merge-base detected
- [x] `--weight diff` mode: frame weight = lines touched by diff, normalized per response
- [x] HTML renderer: self-contained file, collapsible tree, weight bars, confidence styling, zoom toggle; no network requests; path printed to stdout
- [x] Diff golden tests pass: `test_parse_diff_ranges` + `test_diff_frame_classification` (2 tests)

## M4 — Distribution + skill

- [x] Tier-2 grammars compiled in (Java, Ruby, C, C++); Tier 3 (Bash) parse + defs only
- [x] `--grammar-path <dir>` dynamic loading via libloading; expects `<lang>.so` + `<lang>.scm`
- [x] Release builds: macos-arm64 16 MB (< 60 MB target ✓); linux targets via cross-compile
- [x] `skill/` package: `SKILL.md` protocol + `ensure-borescope.sh`; degrades gracefully when binary absent
- [x] `docs/agent-contract.md` written; JSON schema 1 documented
- [x] `docs/eval.md`: harness design + baseline estimates (harness implementation deferred)
- [x] README with S1–S6 scenario recipes; all commands documented
