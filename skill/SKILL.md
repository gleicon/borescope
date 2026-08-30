# Borescope — Code Navigation Skill

Static call-path engine. Answers "what calls what?" from the indexed codebase without running code.
Use it before reading source files — it is faster and dramatically token-cheaper than grep-and-read.

---

## Setup

### Read project memory first

```bash
borescope memo              # project decisions, danger zones, who worked where
borescope memo --who <dir>  # find who has context on a specific area
```

Run this before indexing when working in an unfamiliar repo or area. `memo` reads `.borescope/memo.toml` (team-committed decisions) and `.borescope/worklog.toml` (auto-generated recent activity). If `worklog.toml` is missing, run `borescope memo --update` to generate it.

### One-time index (run once per repo, re-run after large merges)

```bash
borescope index --no-git    # Phase 1: fast (~seconds); structural queries work immediately
borescope index --git &     # Phase 2: background; needed for hotspots / smells / age / coupled
```

Phase 1 gives you `paths`, `callers`, `map`, `explain`. Phase 2 adds history signals. Both can run
concurrently — Phase 2 layering in while you work is expected behavior, not an error.

---

## Core commands

**Before modifying a function** — find every caller:
```bash
borescope callers <file>:<symbol> --depth 3 -o json
```
Callers with `confidence ≥ 0.7` are certain contracts. Callers below that are guesses — verify.

**To understand code** — read the call tree instead of source:
```bash
borescope paths <file>:<symbol> --depth 4 -o json
```
Open source only for frames you will actually edit.

**Point-to-point path with load signals** — trace entry point → target:
```bash
borescope paths <entry> --to <target> --weight hotspot --analyze
```
`--analyze` emits a `signals` array with `kind`, `severity`, `detail` — cite `detail` directly in
your explanation. Signals: `lock_await`, `blocking_async`, `hot_symbol`, `high_complexity`,
`path_depth`, `cross_file_boundary`, `external_boundary`, `async_handoff`, `unbounded_loop`.

**After modifying** — show what changed and blast radius:
```bash
borescope diff -o json          # structured diff for agents
borescope diff                  # tree view for PR description
```

**When lost in a repo** — start here:
```bash
borescope map --weight hotspot   # file × symbol heatmap, sorted by risk
borescope smells                 # structural + semantic antipattern report
borescope hotspots --top 15      # ranked by churn × recency (hottest production files first)
```

**Plain-English profile**:
```bash
borescope explain <symbol>            # risk verdict + narrative
borescope explain-pr <branch>         # PR impact: risk, blast radius, co-change warnings
```

---

## Diagram output

All query commands support `-o mermaid` and `-o dot`. Output is wrapped in a fenced code block
so it renders immediately in GitHub PRs, Claude Code, Cursor, and any Markdown-aware surface.

| Command | `-o mermaid` emits |
|---|---|
| `paths --to` | `sequenceDiagram` (call chain A→B→C) |
| `paths` | `flowchart TD` (full call tree) |
| `callers` | `flowchart BT` (who calls X, bottom-to-top) |
| `map` | `flowchart TD` (file × symbol heatmap) |
| `coupled` | `flowchart LR` (co-change dependency graph) |
| `smells` | `classDiagram` (files as classes, smell kinds as members) |
| `diff` / `branch` | `flowchart TD` (changed call tree) |

```bash
# Paste-ready diagram into a PR comment or doc
borescope paths api/handler.go:HandleCheckout --to db.go:InsertOrder -o mermaid

# Raw Mermaid syntax for piping (no code fence)
borescope map --weight hotspot -o mermaid --no-fence

# DOT for large graphs — pipe to Graphviz for PNG
borescope map -o dot --no-fence | dot -Tpng -o call-map.png

# Share co-change coupling as a diagram
borescope coupled src/auth.rs -o mermaid
```

Use `-o dot` instead of `-o mermaid` when the graph has many nodes — Graphviz layout is superior
to Mermaid for dense call graphs. Neither format requires a runtime dependency.

---

## Interpreting output

- `confidence < 0.7`: dashed frame (`┄`) — edge is a linker guess, not certain. Verify before relying on it.
- `external: true` node — callee not in the indexed repo (stdlib, OS, unindexed dep). Traversal stops here.
- `cochange` section — files that historically change with the target. Note or update them.
- `signals[].detail` — composable sentence describing the mechanism, not just the label. Cite it directly.
- Exit 3: ambiguous target — pick from the JSON candidates on stderr.
- Exit 4: no index — run `borescope index` and retry.

---

## Token efficiency notes

- Prefer `-o json` for agent consumption of structured data (callers, smells, explain-pr).
- Prefer `-o mermaid` when the output will be shown to a human or pasted into a PR/doc.
- Read source files only for frames in the call tree you will actually change.
- `borescope explain <symbol>` gives you complexity, fanin, hotspot, patterns, and co-change
  partners in ~300 tokens — faster than reading the file.
- `borescope memo` gives architectural context in ~200 tokens — read it before asking questions
  that the team already answered in `memo.toml`.

---

## Installation

### Claude Code

```bash
# Install the skill file (one-time)
mkdir -p ~/.claude/skills/borescope && borescope skill > ~/.claude/skills/borescope/SKILL.md
```

Or copy `skill/SKILL.md` from the repo into `~/.claude/skills/borescope/SKILL.md`.

The skill is loaded automatically by Claude Code when you work in any repo that has a
`.borescope/index.db` file.

### Cursor

```bash
mkdir -p .cursor/rules
borescope skill > .cursor/rules/borescope.md
```

Or add to `.cursor/rules/borescope.mdc` with frontmatter:
```
---
description: Borescope code navigation — use before reading files
globs: ["**/*"]
alwaysApply: true
---
```
Then paste the SKILL.md content below the frontmatter.

### OpenHands / open-source agents

Pass the skill content as additional system prompt context:
```bash
borescope skill   # prints SKILL.md to stdout — pipe into your agent's system prompt
```

For agents that accept a `--system-prompt-file` flag:
```bash
borescope skill > /tmp/borescope-skill.md
agent-cli run --system-prompt-file /tmp/borescope-skill.md "refactor PaymentService.charge"
```

### Any platform

`borescope skill` prints this file to stdout. Redirect it wherever your platform expects skill
or system-prompt files. The content is self-contained Markdown — no external references.
