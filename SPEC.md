# Specification: Borescope

## Problem

AI-generated code produces diffs too large to review line-by-line. Reviewers and coding agents need call-path views — the same abstraction as stack traces and flamegraphs — derived statically from any language, without running the code or having a build environment. No existing tool combines multilingual static call graphs, git history signals (churn, coupling, age), semantic zoom, and an agent-ready JSON contract in a single offline binary.

## Scope

**In scope:**
- Static symbol/call graph indexed from source via tree-sitter, zero config, no build step
- Forward slice (paths), reverse slice (callers), diff slice (diff/branch), overview (map)
- Git history signals: churn, code age, hotspot score, co-change coupling
- Four output formats: ANSI tree, Brendan Gregg folded, JSON (stable agent contract), self-contained HTML
- Confidence scoring on every cross-file edge
- Single offline static binary + devskill package

**Out of scope (v1):**
- Type inference, compiler integration, build system awareness
- IDE plugin, LSP server, editor integration
- Dynamic analysis or runtime tracing
- Hosted service, telemetry, or network calls
- MCP server wrapper
- Data-flow / taint analysis
- Windows as a release target (avoid Unix-only assumptions in code)

## Users

**Human reviewers** — need to review PRs as call-path diffs instead of line diffs; navigate large unfamiliar codebases from a terminal without an IDE.

**Coding agents** — need compact weighted call trees as context (fewer tokens than raw source); need blast-radius queries before and after editing.

## Functional Requirements

FR-1: The system SHALL index a repository into a symbol/call graph when invoked with `borescope index`, requiring no compiler, build system, or configuration file.

FR-2: The system SHALL answer a forward-slice query (`paths`) returning all statically reachable call paths from a named symbol when given a `file:symbol` or `file:line` target.

FR-3: The system SHALL answer a reverse-slice query (`callers`) returning all symbols that call a named target.

FR-4: The system SHALL answer a diff-slice query (`diff` / `branch`) marking each frame as added, removed, or modified relative to a revision pair.

FR-5: The system SHALL answer an overview query (`map`) rendering the containment tree of the repository weighted by a chosen metric.

FR-6: The system SHALL mine git history to compute per-symbol churn, code age, hotspot score, and co-change coupling, exposed via `hotspots`, `coupled`, `age`, and `smells` commands.

FR-7: The system SHALL annotate every cross-file call edge with a confidence score between 0.0 and 1.0 and distinguish certain from possible edges in all renderers.

FR-8: The system SHALL support output formats: ANSI tree (default), Brendan Gregg folded (pipeable to `inferno-flamegraph`), JSON (stable contract), and self-contained HTML.

FR-9: The system SHALL perform incremental indexing — on any command, reparse and relink only files changed since the last index, not the entire repository.

FR-10: The system SHALL detect and report antipatterns (`smells`): shotgun-surgery, god-file, stale-core, and tangled-pair, each with a verifiable definition.

FR-11: The system SHALL ship a devskill package (`skill/`) that teaches a coding agent a protocol: when to run which commands, how to interpret confidence scores, and how to recover from error exit codes.

FR-12: The system SHALL support at minimum Go, Rust, Python, TypeScript/TSX, and JavaScript as Tier 1 languages with high extraction precision, and Java, Ruby, C, C++ as Tier 2.

FR-13: The system SHALL accept a `--weight` flag selecting: `none`, `loc`, `fanin`, `churn`, `hotspot`, or `diff`; all weights normalized 0..1 within a single response.

FR-14: The system SHALL accept targets in three forms: `path/to/file:Symbol`, `path/to/file:line_number` (resolves to enclosing symbol), and `QualifiedName` (exits with code 3 and candidate list if ambiguous).

FR-15: The system SHALL add `.borescope/` to `.gitignore` automatically on first index without prompting.

## Non-Functional Requirements

NFR-1: Latency — cold `index` on a 300k-LOC repo with no git mining SHALL complete in under 30 s on an 8-core machine.

NFR-2: Latency — cold `index --git` with 10k commits SHALL add under 20 s to NFR-1.

NFR-3: Latency — incremental index of 10 changed files SHALL complete in under 1 s.

NFR-4: Latency — `paths` or `callers` at depth 4 SHALL return in under 200 ms.

NFR-5: Latency — `diff` on a 50-file PR SHALL return in under 2 s.

NFR-6: Size — release binary with Tier 1+2 grammars SHALL be under 60 MB.

NFR-7: Memory — peak RSS during a 1M-LOC cold index SHALL stay under 2 GB.

NFR-8: Availability — fully offline; zero network calls at any point.

NFR-9: Robustness — indexing SHALL succeed on code that does not compile; partial or broken files must produce partial output, not a crash.

NFR-10: Precision — on Tier 1 languages, confidence-≥0.7 call edges SHALL achieve ≥ 0.9 precision against hand-written ground truth fixture repos.

## Interfaces

**CLI commands (entry points):**
- `borescope index` — build or update `.borescope/`
- `borescope paths <target>` — forward slice; `--analyze` appends signals array
- `borescope callers <target>` — reverse slice
- `borescope diff [rev1 [rev2]]` — call-tree diff
- `borescope branch <name>` — sugar for diff from merge-base
- `borescope map` — repo overview
- `borescope hotspots` — ranked churn × complexity
- `borescope coupled <target>` — co-change partners
- `borescope age` — code-age view
- `borescope smells` — antipattern report
- `borescope explain <target>` — plain-English risk profile for a symbol
- `borescope explain-pr <branch>` — PR impact: risk, blast radius, co-change warnings
- `borescope skill` — print embedded skill file to stdout (for Claude Code, Cursor, agents)

**Output formats at boundaries:**
- `tree` — ANSI to stdout, `--no-color` for plain ASCII
- `folded` — Brendan Gregg one-line-per-path to stdout; integer weights ×1000
- `json` — versioned (`"schema": 1`) to stdout; additive-only evolution; unknown fields ignored
- `html` — self-contained file written to disk; path printed to stdout

**External systems:**
- `git` CLI (shells out; no libgit2 dependency): `log --numstat`, `diff --name-only`, `merge-base`, `cat-file`
- `tree-sitter` grammars (compiled in for curated set; `--grammar-path` for extras)

**Storage:**
- `.borescope/index.db` (SQLite) — all graph data, git stats, file hashes, schema version

## Constraints

- Language: Rust, single cargo workspace, one static binary output
- No network calls anywhere in the binary
- No build system or compiler required to index a repo
- Grammar ABI must be locked to the pinned tree-sitter runtime version
- JSON output schema must be stable under `"schema": 1`; breaking changes require a new schema version
- Deleting `.borescope/` and reindexing must always restore a correct graph

## Acceptance Criteria

AC-1: Given a Go repository with no build environment, when `borescope index` is run, then exit code is 0 and `.borescope/index.db` exists with non-zero symbol count.

AC-2: Given an indexed repo, when `borescope paths file.go:FuncName` is run, then output contains FuncName as root and all directly called symbols appear as children at depth 1.

AC-3: Given an indexed repo, when `borescope callers file.go:FuncName` is run, then every listed caller actually calls FuncName (verified against ground-truth fixture).

AC-4: Given two commits A and B with known added/removed/modified functions, when `borescope diff A B` is run, then each changed frame is prefixed with the correct `+`, `-`, or `~` marker.

AC-5: Given an indexed repo, when `-o folded` is passed, then `borescope paths X -o folded | inferno-flamegraph > x.svg` produces a valid SVG with exit code 0.

AC-6: Given an indexed repo, when `-o json` is passed, then output is valid JSON containing `"schema": 1` and a `root` object with `id`, `name`, `qualified`, `file`, `span`, `weight`, and `children` fields.

AC-7: Given a repo with git history, when `borescope hotspots` is run, then the top result is a symbol with high churn × complexity; churn and hotspot values match recomputed ground truth within ±1 commit.

AC-8: Given a repo with known co-change pairs (synthetic scripted history), when `borescope coupled <file>`, then all pairs at strength ≥ 0.3 and support ≥ 5 appear in output.

AC-9: Given a repo with a god-file (p95 LOC + p95 fan-in), when `borescope smells` is run, then that file appears in the `god-file` section.

AC-10: Given a 300k-LOC polyglot repo on an 8-core machine, when `borescope index` is run cold, then it completes in under 30 s (measured wall time, release build).

AC-11: Given 10 files changed since last index, when any borescope command is run, then only those 10 files (and their dependents) are reparsed, completing in under 1 s.

AC-12: Given a file with syntax errors, when `borescope index` is run, then exit code is 0 and valid symbols from the file appear in the graph (partial extraction, no crash).

AC-13: Given a target that matches multiple symbols, when `borescope paths QualifiedName` is run, then exit code is 3 and stderr contains a JSON array of candidate qualified names.

AC-14: Given a fresh machine with only `git` installed, when all M0–M4 README demos are followed, then each demo command exits 0 and produces non-empty output.

AC-15: Given a skill-equipped agent completing a defined refactor task on a fixture repo, when compared against the same agent without the skill, then the skill agent reads measurably fewer source bytes (recorded in `docs/eval.md`).

**FR → AC coverage:**
- FR-1 → AC-1, AC-12
- FR-2 → AC-2
- FR-3 → AC-3
- FR-4 → AC-4
- FR-5 → AC-2 (map uses same query engine)
- FR-6 → AC-7, AC-8
- FR-7 → AC-6 (confidence in JSON), AC-2 (dashed rendering)
- FR-8 → AC-5, AC-6
- FR-9 → AC-11
- FR-10 → AC-9
- FR-11 → AC-15
- FR-12 → AC-1 (Go), AC-10 (polyglot)
- FR-13 → AC-6 (weight field in JSON)
- FR-14 → AC-13
- FR-15 → AC-1 (`.gitignore` check after first index)

## Open Questions

OQ-1: Symbol-span git attribution — full `git blame` per symbol vs. hunk-overlap approximation from `log -p`? (Hunk overlap is cheaper; blame is O(file × commits).)

OQ-2: Should `cochanges` edges be symbol-granularity in v1, or file-level only? (File-level is acceptable for v1 if symbol-level lift is costly.)

OQ-3: `--zoom stmt` (statement-level frames, calldiff-style `if/else` rendering): target M3 or v1.1?

OQ-4: Windows support: not a v1 release target, but which Unix-only assumptions would block a future port?
