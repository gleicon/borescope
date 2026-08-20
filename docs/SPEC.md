# Borescope — Product & Technical Specification

**Version:** 0.4.1
**Status:** Implemented (M0–M4 complete)
**Target:** Single Rust binary + devskill package

---

## 1. Problem Statement

AI-generated code has broken line-by-line code review. Reviewers cannot read a 2,000-line diff produced by an agent; they need the same abstraction they use when production code breaks: **stack traces and flamegraphs** — but derived *statically*, without running the code, and available in every language (not just those with pprof-class tooling).

Borescope is a **static call-path engine**. It parses a repository into a symbol/call graph via tree-sitter, enriches it with git-history signals (churn, age, co-change coupling), and answers path queries rendered as zoomable, weighted call trees — "a photograph of what a profiler would show," derived from structure instead of execution.

It serves two consumers with the same engine:

1. **Humans** — reviewing PRs as call-path diffs instead of line diffs; navigating unfamiliar or large codebases from a terminal, no IDE.
2. **Coding agents** — consuming compact, weighted call trees as context (far fewer tokens than raw source) and using blast-radius queries before/after editing.

### Prior art and differentiation

| Tool | What it does | Borescope's delta |
|---|---|---|
| `calldiff` (npx, Oxc-based) | Call-stack diff of changed JS/TS functions, HEAD → worktree | Multilingual, weighted frames, semantic zoom, forward/reverse slicing, git signals, agent contract |
| ast-grep | Language-generic AST search via tree-sitter | Borescope builds a persistent *graph* and answers path queries, not pattern matches |
| Aider repo map | PageRank symbol map for LLM context | Borescope gives *paths with weights and confidence*, queryable per symbol |
| CodeScene / Tornhill | Hotspots, change coupling from git | Borescope fuses these signals *into* the call graph and CLI workflow |
| SCIP / LSIF / stack graphs | Precise cross-file resolution | Optional precision booster; borescope works without compilers or build steps |

---

## 2. Goals and Non-Goals

### Goals (v1)

- G1. Index any repo in tree-sitter-supported languages into a local symbol/call graph with **zero configuration and no build step** (works on code that doesn't compile).
- G2. Answer four query shapes: forward slice (`paths`), reverse slice (`callers`), diff slice (`diff`, `branch`), and overview (`map`).
- G3. Mine git history for churn, code age, hotspot score, and co-change coupling; expose them as frame **weights** and as standalone commands (`hotspots`, `coupled`, `age`, `smells`).
- G4. Render as ANSI tree (default), Brendan-Gregg folded format (pipeable to `flamegraph.pl`/`inferno`), machine JSON (stable agent contract), and static HTML.
- G5. Honest imprecision: every cross-file edge carries a confidence score; renderers distinguish certain from possible edges.
- G6. Ship as one static binary (curated grammar set compiled in) + a devskill directory that wraps it for coding agents.

### Non-Goals (v1)

- NG1. No type inference, no compiler integration, no build-system awareness. (SCIP import is a v2 booster.)
- NG2. No IDE plugin, LSP server, or editor integration.
- NG3. No dynamic analysis, instrumentation, or runtime tracing.
- NG4. No hosted service, telemetry, or network calls. Fully offline.
- NG5. No MCP server in v1 (design the JSON contract so an MCP wrapper is trivial in v2).
- NG6. Data-flow analysis (taint, value tracking). Call/containment/co-change edges only.

---

## 3. Users and Core Scenarios

### S1 — PR review (human)
"Show me this PR as call paths, not lines."
```
borescope diff main HEAD --weight hotspot
```
Output: call-tree diff where added/removed/modified frames are marked `+`/`-`/`~`, frame emphasis proportional to historical risk (hotspot) of the touched code.

### S2 — Entry-point exploration (human)
"From this HTTP handler, what can execute?"
```
borescope paths api/checkout.go:HandleCheckout --depth 4 --weight fanin
```
Output: all statically reachable call paths, widest (highest fan-in) first, dashed frames for low-confidence edges.

### S3 — Blast radius before editing (agent)
"What might break if I change this function?"
```
borescope callers internal/auth/token.go:Verify -o json
```
Agent receives: certain callers, possible callers with confidence, plus co-change-coupled files ("historically changes with X 85% of the time").

### S4 — Context instead of source (agent)
Skill instructs the agent: when asked to explain or modify code, fetch `borescope paths <target> -o json --depth 3` and reason over the weighted tree (~50 lines) instead of reading files (~2,000 lines).

### S5 — Repo archaeology (human)
"Where are the antipatterns in this old codebase?"
```
borescope smells
borescope coupled src/billing/invoice.py --min 0.5
borescope age --zoom pkg
```

### S6 — Branch visualization (human)
"What does this branch actually touch?"
```
borescope branch feature/new-pricing
```
Same as `diff merge-base(main, branch)..branch`, presented as a single impact tree.

---

## 4. Architecture

Single cargo workspace, one binary crate + library crates:

```
borescope/
├── crates/
│   ├── bs-core/        # graph model, store, query engine
│   ├── bs-extract/     # tree-sitter parsing + tags-based extraction
│   ├── bs-link/        # cross-file resolution (imports + names + confidence)
│   ├── bs-git/         # git history miner (churn, age, co-change)
│   ├── bs-render/      # tree / folded / json / html renderers
│   └── bs-cli/         # clap-based CLI, the `borescope` binary
├── grammars/           # vendored grammar crates or build glue
├── skill/              # devskill package (SKILL.md + helpers)
└── testdata/           # fixture repos per language
```

### Pipeline

```
source files ──▶ bs-extract ──▶ defs, refs, calls, imports (per file)
                                        │
git log ───────▶ bs-git ──▶ churn, age, co-change matrix
                                        │
                 bs-link ◀──────────────┘
                    │  resolve refs → defs (confidence-scored edges)
                    ▼
              bs-core graph store (.borescope/)
                    ▼
              query engine (paths / callers / diff / map / hotspots …)
                    ▼
              bs-render (tree | folded | json | html)
```

### Key dependency choices

| Concern | Choice | Notes |
|---|---|---|
| Parsing | `tree-sitter` runtime (Rust binding), pin one runtime version (0.26.x line); vendor grammar crates and lock them (grammar ABI must match runtime) | Robust to syntax errors — must index half-broken agent output |
| Grammars | Statically compiled curated set (see §5) + `--grammar-path` dynamic loading escape hatch for extras | Single-binary story first |
| Git | Shell out to `git` (`log --numstat`, `diff --name-only`, `merge-base`, `cat-file`) rather than libgit2/gitoxide for v1 | Simpler, matches every repo's git version; abstract behind a trait so gitoxide can replace it later |
| CLI | `clap` (derive) | |
| Storage | `rusqlite` (bundled SQLite) in `.borescope/index.db` | Boring on purpose; see §8. Schema versioned; swapping the backend later must not change CLI or JSON output |
| Parallelism | `rayon` for per-file parse/extract | |
| Folded/flamegraph | Emit text; do **not** bundle a flamegraph renderer. Optionally suggest `inferno` in docs | |
| HTML | Single self-contained file, inline JS/CSS, no CDN | Collapsible tree; d3-free hand-rolled is fine |

---

## 5. Language Support

### Extraction strategy

Language-generic extraction driven by **tree-sitter queries**, ast-grep style. For each language, borescope needs one query pack with these capture classes:

- `@def.function`, `@def.method`, `@def.type` (+ name capture)
- `@ref.call` (+ callee name capture; method calls capture receiver text when present)
- `@import` (module path + alias)

Seed the packs from each grammar's published `tags.scm` (the queries GitHub code-nav uses), then patch per language as needed. Query packs are embedded in the binary but overridable via `--query-path` for debugging.

### v1 curated grammar set (statically linked)

Tier 1 (must pass full test matrix): **Go, Rust, Python, TypeScript/TSX, JavaScript**
Tier 2 (extraction + linking, relaxed precision targets): **Java, Ruby, C, C++**
Tier 3 (parse + defs only is acceptable): **Bash, HCL, YAML** (YAML/HCL contribute *nodes* for co-change coupling, not call edges)

Everything else: dynamic grammar loading via `--grammar-path <dir>` with a user-supplied query pack; document the pack format.

### Per-language linking notes

- **Go**: imports are explicit; package-qualified calls resolve with high confidence. Method calls resolve by name within candidate types (medium confidence).
- **Rust**: `use` graph + module tree resolution; trait-method calls resolve to all implementors (each edge medium confidence, annotated `via trait`).
- **Python/JS/TS/Ruby**: import-aware name resolution; unresolved receivers fall back to global name match (low confidence). Duck typing produces *candidate* edges by design.
- **C/C++**: include graph is heuristic; function-pointer calls are out of scope (emit `unresolved` markers).

---

## 6. Data Model

### Nodes

```
Symbol {
  id:          u64 (stable hash of file_path + qualified_name + kind)
  kind:        function | method | type | module | package | file | config_node
  name:        String            // "Verify"
  qualified:   String            // "internal/auth.Token.Verify"
  file:        PathBuf
  span:        (start_line, end_line)
  lang:        LanguageId
  // git-derived (nullable until `index --git` runs)
  churn:       u32               // commits touching this span (via line-range attribution)
  age_days:    u32               // days since last change to span
  loc:         u32
  complexity:  u32               // cheap proxy: max nesting depth × branch-node count (language-generic from AST)
  hotspot:     f32               // normalized churn × complexity, 0..1
}
```

File-level git stats are always available; span-level attribution (blame-based) is computed lazily on first query that needs it, then cached.

### Edges

```
Edge {
  from: SymbolId
  to:   SymbolId
  kind: calls | contains | imports | cochanges
  confidence: f32   // 0..1, see below
  meta: { count?: u32, support?: u32, via?: String }   // co-change stats, "via trait", etc.
}
```

**Confidence rubric (calls):**

| Situation | Confidence |
|---|---|
| Same-file resolution | 1.0 |
| Import-qualified, unique target | 0.9 |
| Unique name match within imported modules | 0.7 |
| Method name match on candidate types (multi-target) | 0.5 per edge |
| Global name match, multiple candidates | 0.3 per edge |
| Unresolvable (dynamic dispatch, metaprogramming) | edge to synthetic `<unresolved: name>` node, 0.1 |

**Co-change edges** (file↔file, lifted to symbol level where spans allow):

- `support` = commits touching both A and B
- `strength` = support / min(commits(A), commits(B))  — asymmetric variant P(B|A) also stored
- Default reporting threshold: `strength ≥ 0.3` and `support ≥ 5`; both configurable.
- Exclude merge commits and commits touching > 50 files (mass renames/formatting) by default.

---

## 7. CLI Specification

Binary name: `borescope`. All commands assume cwd is inside a git repo unless `--repo <path>` is given. All commands auto-run a fast incremental index check first (see §8) unless `--no-index` is passed.

### Global flags

```
--repo <path>          repo root (default: discovered via .git)
--depth <n>            max tree depth (default: 3 for paths/callers, unlimited for diff)
--zoom <level>         pkg | mod | fn | stmt   (default: fn)
--weight <w>           none | loc | fanin | churn | hotspot | diff   (default: none;
                       diff commands default to `diff`)
--min-confidence <f>   hide edges below threshold (default: 0.0 = show all, styled)
-o, --output <fmt>     tree | folded | json | html   (default: tree; html writes a file
                       and prints its path)
--no-color             plain ASCII tree
-q / -v                quiet / verbose
```

### Commands

```
borescope index [--full] [--git] [--grammar-path <dir>]
    Build or incrementally update .borescope/. --full forces reindex.
    --git (default on) runs the history miner.

borescope paths <target> [flags]
    Forward slice: everything reachable from <target>.
    <target> forms:  path/to/file.go:FuncName
                     path/to/file.go:42            (line → enclosing symbol)
                     QualifiedName                 (unique match required, else list candidates and exit 3)

borescope callers <target> [flags] [--coupled]
    Reverse slice. --coupled appends the co-change section (on by default in json).

borescope diff [<rev1> [<rev2>]] [flags]
    Call-tree diff. Defaults: rev1=HEAD, rev2=worktree.
    Frames marked: + added   - removed   ~ signature/body modified   (context frames unmarked)
    A frame is included if it, or any descendant within --depth, changed.

borescope branch <name> [--base <rev>]
    Sugar for: diff $(git merge-base <base|main> <name>) <name>

borescope map [--zoom pkg|mod] [flags]
    Repo overview: containment tree weighted by chosen metric.

borescope hotspots [--top <n>]
    Ranked churn × complexity table.

borescope coupled <file|target> [--min <strength>] [--support <n>]
    Co-change partners of a file or symbol.

borescope age [--zoom pkg|mod|fn]
    Code-age view (containment tree, weight = staleness).

borescope smells
    Antipattern report:
      shotgun-surgery : symbol/file with ≥ 4 co-change partners at strength ≥ 0.5
      god-file        : file > p95 loc AND > p95 fan-in
      stale-core      : age > 2y AND fan-in > p90 (old code everything depends on)
      tangled-pair    : A↔B strength ≥ 0.8 with no import/call edge between them

borescope query <sexp|json>       (v1.1, reserved)
    Raw graph query escape hatch for agents.
```

### Exit codes

```
0 success · 1 runtime error · 2 usage error · 3 ambiguous target (candidates listed on stderr as JSON)
4 index missing/corrupt (with hint) · 5 grammar unavailable for requested file
```

---

## 8. Storage — `.borescope/`

```
.borescope/
├── index.db        # SQLite: symbols, edges, files, git_stats, cochange, meta
├── queries/        # (optional) user query-pack overrides
└── VERSION
```

- `meta` table stores: schema_version, runtime/grammar versions, indexed HEAD sha, per-file content hashes.
- **Incremental indexing:** on every command, compare file hashes (respecting `.gitignore`); reparse/re-link only changed files and their dependents (files importing them). Full relink is O(changed neighborhood), not O(repo).
- Git miner is incremental by last-seen commit sha.
- `.borescope/` must be added to `.gitignore` automatically on first index (prompt-free; print notice).
- Hard requirement: deleting `.borescope/` and re-running any command must always work.

---

## 9. Output Formats

### 9.1 `tree` (default, ANSI)

```
PiService.createAgentSession(options)                          ██████ 0.82
├─ PiService.getServices()                                     ████   0.61
│  ├─ SettingsManager.create()                                 ▌      0.07
│  ├─ AuthStorage.create()                                     █      0.12
│  └─ createCodingTools()                                      ██     0.29
├─ services.boot()                                             ▌      0.05
└─ SessionManager.open(_id)   ┄┄ 0.5 via trait                 █      0.15
```

- Right column: weight bar + normalized value (omitted when `--weight none`).
- Low-confidence edges: dashed connector `┄┄` + confidence annotation.
- Diff mode prefixes `+ / - / ~` with green/red/yellow.
- Depth-collapsed nodes show `▸ (12 more)`.

### 9.2 `folded` (Brendan Gregg format)

One line per root-to-leaf path, `;`-separated frames, integer weight (scaled ×1000):

```
HandleCheckout;validateCart;PriceService.quote 820
HandleCheckout;validateCart;Inventory.check 410
```

Guarantee: `borescope paths X -o folded | inferno-flamegraph > x.svg` works with no massaging.

### 9.3 `json` — the agent contract (stability guaranteed)

```json
{
  "borescope": "0.1.0",
  "schema": 1,
  "query": { "cmd": "callers", "target": "internal/auth/token.go:Verify",
             "depth": 3, "weight": "hotspot" },
  "root": {
    "id": "a3f9…",
    "name": "Verify",
    "qualified": "internal/auth.Token.Verify",
    "file": "internal/auth/token.go",
    "span": [41, 88],
    "weight": 0.82,
    "signals": { "churn": 34, "age_days": 12, "loc": 47, "hotspot": 0.82 },
    "children": [
      { "id": "…", "name": "HandleLogin", "edge": { "kind": "calls",
        "confidence": 0.9 }, "children": [] }
    ]
  },
  "cochange": [
    { "file": "internal/auth/middleware.go", "strength": 0.85, "support": 17 }
  ],
  "truncated": { "depth": true, "nodes_omitted": 12 },
  "unresolved": [ { "name": "dispatch", "site": "router.go:88" } ]
}
```

Rules: additive-only evolution under `schema: 1`; unknown fields must be ignored by consumers; `weight` always normalized 0..1 within the response.

### 9.4 `html`

Self-contained file: collapsible tree, weight bars, confidence styling, client-side zoom-level toggle. No network requests. Written to `borescope-<cmd>-<ts>.html`, path printed to stdout.

---

## 10. Weights

All weights normalized 0..1 within a single response (max-scaled).

| `--weight` | Definition |
|---|---|
| `loc` | lines of the symbol |
| `fanin` | count of incoming `calls` edges (confidence-weighted sum) |
| `churn` | commits touching the symbol's span |
| `hotspot` | `norm(churn) × norm(complexity)` — Tornhill-style |
| `diff` | lines of the symbol touched by the diff under review (diff/branch commands) |
| `none` | uniform |

`complexity` (language-generic, no per-language rules): count of branch-type named nodes (`if / for / while / match / case / catch / &&`-style, mapped per grammar in the query pack) plus max nesting depth. Crude by design; it only needs ordinal validity.

---

## 11. Devskill Package (`skill/`)

Ships alongside the binary, same model as tldt: the skill is a thin protocol wrapper, the binary does the work.

```
skill/
├── SKILL.md
└── scripts/ensure-borescope.sh   # locate or install binary; verify version ≥ min
```

`SKILL.md` teaches the agent a **protocol**, not just commands:

1. **Before modifying** a function: `borescope callers <target> -o json` → treat certain callers as contract; mention possible callers and co-changed files in the plan.
2. **To understand** code: prefer `borescope paths <target> -o json --depth 3` over reading files; only open source for frames you'll actually edit.
3. **After modifying**: `borescope diff -o json`; include the rendered `tree` output in the PR description / final report.
4. **When lost** in a repo: `borescope map --zoom mod --weight hotspot` first, then drill.
5. Interpret `confidence < 0.7` edges as "verify before relying on this."
6. If exit code 4: run `borescope index` and retry. If exit 3: pick from the candidates JSON.

Acceptance: skill must degrade gracefully (clear message, no crash loops) when the binary is absent and the install script fails.

---

## 12. Performance Targets

Measured on the fixture matrix + three real repos (~50k, ~300k, ~1M LOC), release build, 8-core dev machine:

| Operation | Target |
|---|---|
| Cold `index` (300k LOC, no git) | < 30 s |
| Cold `index --git` (10k commits) | + < 20 s |
| Incremental index (10 changed files) | < 1 s |
| `paths` / `callers` depth 4 | < 200 ms |
| `diff` on a 50-file PR | < 2 s |
| Binary size (tier 1+2 grammars) | < 60 MB |
| Peak RSS during 1M-LOC index | < 2 GB |

---

## 13. Testing Strategy

- **Fixture repos** per tier-1 language in `testdata/`: hand-written mini-repos with a known ground-truth call graph (checked-in JSON). Extraction/link tests assert precision and recall against ground truth: tier 1 ≥ 0.9 precision on confidence ≥ 0.7 edges; recall reported, not gated.
- **Golden-output tests** for every renderer (tree with `--no-color`, folded, json) via snapshot testing (`insta`).
- **Git-miner tests** against a scripted synthetic history (fixed commits generated by a test script → deterministic churn/coupling numbers).
- **Diff tests**: fixture repo with two branches; assert `+/-/~` classification.
- **Property test**: for any indexed repo, every edge's endpoints exist; deleting `.borescope/` and reindexing yields an isomorphic graph.
- **Smoke matrix** in CI: index tree-sitter's own repo, ast-grep, and one Go project; assert exit 0 and non-empty graph.

---

## 14. Milestones

Each milestone ends with a demoable command and its tests.

**M0 — Git miner (standalone value)**
`bs-git` + `bs-core` store + `hotspots`, `coupled`, `age`, `smells`, `map --weight churn` (file/dir zoom only). No tree-sitter yet. *Demo: antipattern report on a real repo.*

**M1 — Extraction**
`bs-extract` with tier-1 grammars, query packs, `index`, and `map --zoom fn` (containment only). *Demo: symbol inventory of a mixed-language repo, including files with syntax errors.*

**M2 — Linking + slicing**
`bs-link` confidence model, `paths`, `callers`, tree + folded + json renderers, span-level git attribution (weights on frames). *Demo: S2 and S3 scenarios end-to-end.*

**M3 — Diff**
`diff`, `branch`, `--weight diff`, HTML renderer. *Demo: S1 — review a real AI-generated PR as a weighted call-tree diff.*

**M4 — Distribution + skill**
Tier-2 grammars, `--grammar-path` dynamic loading, static release builds (linux-x64/arm64, macos-arm64), `skill/` package, README with the S1–S6 recipes. *Demo: a coding agent completing a task using the skill protocol.*

---

## 15. Open Questions (decide during implementation, do not block M0–M1)

1. Symbol-span git attribution: full `git blame` per symbol vs. hunk-overlap approximation from `log -p`? (Start with hunk overlap; blame is expensive.)
2. Should `cochanges` edges be lifted to symbol granularity in v1, or file-level only with symbol lift in v1.1? (File-level acceptable for v1 if symbol lift is costly.)
3. `--zoom stmt` (statement-level frames, matching calldiff's `if/else` rendering): M3 or v1.1?
4. Windows support: not a v1 release target, but avoid Unix-only assumptions in code.

---

## 16. Definition of Done (v1)

- All M0–M4 demos reproducible from the README on a clean machine with only `git` installed.
- `borescope diff` on a 500-file PR of a 300k-LOC polyglot repo is *usable*: correct classifications, < 2 s, output shorter than the raw diff.
- JSON schema 1 documented in `docs/agent-contract.md` and covered by golden tests.
- An agent following `SKILL.md` measurably reads fewer source bytes to complete a defined refactor task on a fixture repo than the same agent without the skill (record the harness + numbers in `docs/eval.md`).