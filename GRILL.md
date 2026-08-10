# GRILL.md — Borescope design decisions

---

## Milestones

### M0-fixes (no tree-sitter, implement now)
- D3: Hotspot recency decay in miner.rs
- D4: Artifact file filter in git miner
- D10: `--weight diff` validation + `Weight::Fanin`
- D14: `DEFAULT_MIN_SUPPORT = 5` across all commands

### M1 (extraction)
- D5: Content-hash incremental indexing (`file_hash` column + skip logic)
- D13 Level 1: Runtime `.borescope/queries/<lang>.scm` override
- D12a: `map --top N` (default 50)

### M2 (linking + query)
- D1: Cross-language linker filter by `lang`
- D9: Import-path narrowing in linker + Tier-1 fixture repos
- D2: Path-confidence product threshold
- D11: External callees as first-class nodes (`external: true`)
- D12b: Smells semantic SQL pre-filter

### M3 (diff + analysis)
- D7: Diff hunk-polarity (`+`/`-`/`~`) classification
- D15: `paths --to --analyze` + LLM-legible `signals` JSON
- D6: `.borescope/thresholds.toml` per-language risk config

### M4 (release)
- D8: Two-phase cold-start documented in skill
- D9: Fixture repos precision gate (≥0.9 on confidence-≥0.7 edges)
- D13 Level 2: `.borescope/smells.toml` custom rules

---

## D15 — Point-to-point path query + LLM-legible signal output

**Question:** Should borescope support querying the path from symbol A to symbol B, and if so, what should the output look like?

**Answer:** `borescope paths <from> --to <target> --analyze` finds all paths from A to B (bidirectional BFS, pruned to paths reaching target). `--analyze` adds a top-level `signals` array to JSON output. Each signal has: `kind`, `severity` (high/medium/low), `symbol`, `file`, `depth`, and a `detail` field written as a composable LLM-legible sentence describing the *mechanism*, not just the symptom. Borescope pre-ranks signals by severity so the LLM leads with what matters.

**Rationale:** Static analysis cannot model load — but it can surface structural signals (lock_across_await, spawn_in_loop, fanin_bottleneck) with their mechanistic implications. An LLM running within a skill session has training knowledge to synthesize "what happens at 1000 requests" from those signals. The `detail` field must encode the mechanism ("holds mutex for full DB latency — contention grows linearly with slow requests") not just the label ("lock_across_await"), so the LLM can use it directly without additional inference. The skill teaches agents: "`signals[].detail` is a composable statement about behavior under load — cite it when explaining risk."

---

## D14 — min_support consistency

**Question:** Should `smells`, `explain`, and `explain_pr` use the same co-change minimum support threshold?

**Answer:** Yes. One constant `DEFAULT_MIN_SUPPORT = 5`, shared across all three commands. Override via `--min-support N` on `smells` and `explain`. Current inconsistency (`smells=5`, `explain/explain_pr=3`) means agents comparing output from two commands see different co-change sets for the same file.

**Rationale:** Consistency matters more than the exact number. Different thresholds across commands with no documentation of why is a silent correctness trap for agents doing multi-command analysis.

---

## D13 — User-defined query extensions (custom pattern captures + smell rules)

**Question:** Should users be able to add custom pattern captures and smell rules without rebuilding the binary?

**Answer:** Two levels. Level 1 (now): runtime `.borescope/queries/<lang>.scm` files — `lang_config()` checks for these at startup and appends to built-in query source. Level 2 (M3): `.borescope/smells.toml` declaring custom combination rules (patterns + message). Docs/recipes ship alongside — model is exactly like linters (clippy, ESLint, semgrep): built-in rules + team-local rules + community rule sets. TSX/React hook patterns (useEffect + spawn → stale closure) is the first recipe example.

**Rationale:** Built-in `.scm` files compiled in via `include_str!` mean every new pattern requires a binary rebuild and release. Runtime override directory costs one function change in `lang_config()` and unlocks community-contributed rule sets. Teams grow their own rules the same way they grow their own lint configs.

---

## D12 — map --top N and smells SQL pre-filtering

**Question:** How should `map` and `smells` handle large repos without flooding output or RAM?

**Answer:** `map`: add `--top N` (default 50), limit file nodes by weight before rendering, show `(+M more)` footer. `smells` semantic detector: push pre-filter to SQL — `WHERE patterns != '' AND (hotspot > threshold OR churn > median)` — so only hot symbols with patterns reach the Rust detector. God-file and shotgun surgery detectors operate on aggregate file stats, already fine.

**Rationale:** A 10k-function repo produces a 10k-line `map` tree — unusable in a terminal and token-expensive for agents. A 1M-LOC `smells` run loads 100k symbols + 500k edges into RAM for one query. Both need a scope gate before the data hits Rust.

---

## D11 — External callees as first-class nodes

**Question:** Should unresolved external calls (stdlib, vendored deps) be silently dropped, counted, or shown as tree nodes?

**Answer:** Show as first-class `TreeNode` with `external: true`, `confidence: 0.0`, `unresolved_reason: "external" | "ambiguous"`. Keep `unresolved_edges` count at top level of `JsonOutput` for quick agent consumption. After D9 (import-path narrowing), ambiguous cases should drop significantly; external cases are permanent (no build step = no dep indexing).

**Rationale:** Silently dropping external calls makes the call tree incomplete — an agent sees `main → processRequest` but not `processRequest → http.Get` (external). With external nodes visible, the tree shows the full blast radius including external surface, and agents know where to stop traversing. `unresolved_reason` distinguishes "this is a library boundary" from "this is ambiguous and might be resolved after D9."

---

## D10 — --weight diff on non-diff commands

**Question:** What should happen when `--weight diff` is passed to `paths`, `callers`, or `map`?

**Answer:** Reject at parse time with exit 2 and a clear, actionable error message. Also add `Weight::Fanin` to the Weight enum now (listed in FR-13, currently missing).

**Rationale:** Current behavior silently produces weight=0.0 for every node while printing "score: diff" in the legend — a misleading output that looks valid. Failing loudly with a clear message prevents silent wrong answers. Messages must be explicit (e.g., "`--weight diff` requires a revision pair — use with `diff` or `branch`, not `paths`"), not just exit codes that can be overlooked.

---

## D9 — Import-path narrowing in linker + Tier-1 fixture repos

**Question:** Should the linker use import declarations to narrow resolution candidates before applying the unique/ambiguous confidence fork? And should Tier-1 fixture repos exist before shipping Tier-1?

**Answer:** Yes to both. Linker must use import paths (Go package paths, Python module paths, TS/JS relative imports, Rust `use` paths) to restrict candidates to symbols in the imported scope. Fixture repos (< 500 LOC synthetic, ground-truth call graph JSON checked in) are the release gate for Tier-1 — precision ≥ 0.9 on confidence-≥0.7 edges must pass on the fixture before the language ships.

**Rationale:** In idiomatic Go a service has many functions named `handle` (one per route). Without import narrowing every call resolves at 0.3 across all of them — 7 of 8 results are false positives. A pre-production risk tool built on systematically wrong call graphs defeats its own purpose. NFR-10 must be measurable, not a paper claim.

---

## D8 — Agent cold-start protocol

**Question:** Should agents wait for a full index (symbols + git) before using the tool, or use a two-phase cold-start?

**Answer:** Two-phase: (1) `borescope index --no-git` first — fast, gives paths/callers/map/explain within seconds; (2) `borescope index --git` in background to layer in hotspot/churn signals. Skill must document which commands are safe after phase 1 only (paths, callers, map, explain) vs. requiring phase 2 (hotspots, coupled, smells, age).

**Rationale:** A 300k-LOC repo takes ~30s for full index. Agents blocked on a silent wait fall back to reading files — defeating the purpose. Symbols-only index is fast enough to give immediate structural signal; git signals can be layered in while the agent works.

---

## D7 — Diff frame classification mechanism

**Question:** How should `+`/`-`/`~` markers be derived — by double-indexing two revisions, or by classifying diff hunk polarity?

**Answer:** Hunk-polarity classification. Extend `diff_line_ranges` to track polarity per line (`+`/`-`/mix). A symbol whose span overlaps only `+` lines is `+`; only `-` lines is `-`; mixed is `~`. Renamed functions (old name as `-`, new name as `+` in overlapping span) render as two separate nodes (`-` old, `+` new) — no rename detection without type info.

**Rationale:** Double-index requires worktree manipulation and breaks for files deleted between revisions. Hunk-polarity derives entirely from diff text already being fetched, no second index needed. AC-4 currently unmet — current code only emits `~`, never `+` or `-`.

---

## D6 — Risk threshold configurability

**Question:** Should the thresholds in `risk_level()` (`complexity > 20`, `hotspot > 0.6`, `fanin > 5`) be hardcoded or configurable per language?

**Answer:** Configurable via `.borescope/thresholds.toml` with language-keyed sections and sane defaults matching current hardcoded values.

**Rationale:** Combination rules in `smells.rs` are already language-agnostic (they operate on pattern names from `.scm` captures). The thresholds are not — `fanin > 5` means different things in a Go service (50 functions, one file) vs. a Rust workspace (1000 small modules). Per-language config lets teams tune without code changes.

---

## D5 — Incremental indexing mechanism

**Question:** Should incremental indexing use git's changed-file list or per-file content hashes stored in the DB?

**Answer:** Content-hash (`SHA-256` of file bytes stored as `file_hash TEXT` in the `files` table). Skip re-parse when hash matches.

**Rationale:** Git-based skips files changed outside git (untracked edits, mid-refactor worktree). Content-hash always reflects current disk state, which is the correct ground truth for a static analysis tool. Hashing is cheaper than parsing.

---

## D4 — Artifact file filtering in git miner

**Question:** Should the git miner exclude infra/config/lock files (Terraform, YAML, CloudFormation, *.lock, *.sum) from churn counting and co-change computation?

**Answer:** Yes — filter to `is_source_lang(lang)` using a static allowlist of non-infra LangId variants before accumulating any commit signal. Same filter for both churn/normalization and co-change pair computation.

**Rationale:** Auto-generated and deploy-driven files (Cargo.lock, terraform.yaml, package-lock.json) inflate `max_churn` (compressing all real code hotspot scores downward) and create false co-change edges (e.g., `api.go ↔ terraform.yaml` at strength 0.9) that corrupt smells output. The allowlist is static and decoupled from grammar availability — a file is a source file even if its grammar isn't installed.

---

## D3 — Hotspot recency decay

**Question:** Should hotspot scoring include a recency weight, or stay as raw `churn / max_churn`?

**Answer:** `hotspot = (churn / max_churn) × exp(-0.003 × age_days)`, baked into the miner. λ ≈ 0.003 gives half-life ≈ 230 days.

**Rationale:** Two files with identical total churn but different timing (last week vs. spread over 5 years) carry different risk. The formula is computable entirely from data already in the DB. Keeping it as a single `hotspot` field in JSON preserves the stable schema — consumers don't need to implement the decay themselves.

---

## D2 — Path-confidence threshold

**Question:** Should path traversal cut on per-edge confidence or on the product of all edge confidences along the path from root?

**Answer:** Path-product threshold — prune any subtree whose cumulative path-confidence (product of all edge confidences root→node) falls below `--min-confidence`.

**Rationale:** Per-edge threshold lets a chain `1.0 → 0.7 → 0.3 → 0.7 → 0.7` appear fully in the tree even though the leaf's real confidence is 0.147. Path-product makes the threshold mean what users think it means: "show me only paths I can actually trust to this depth."

---

## D1 — Cross-language linker filtering

**Question:** Should the linker narrow resolution candidates by source language before applying name-based confidence scoring?

**Answer:** Yes — filter candidates to same-`lang` as the caller before the unique/ambiguous fork.

**Rationale:** A polyglot index (Go + Rust + Python in the same DB) would otherwise produce false cross-language edges (e.g., Rust `process` resolving to Python `process` at confidence 0.3), corrupting path queries and smells from day one.

