# Borescope — Code Navigation Skill

Static call-path engine. Answers "what calls what?" from structure, not execution.
**Use before reading source files** — it is faster and token-cheaper than grep-and-read.

---

## Setup (once per repo)

```bash
borescope memo                      # read project decisions + recent activity first
borescope index --no-git            # Phase 1: seconds — paths/callers/map/explain ready
borescope index --git &             # Phase 2: background — hotspots/smells/age/coupled ready
```

`memo` is always safe to run even without an index. Run it first when entering an unfamiliar repo or area.

---

## Scenario: starting on an unfamiliar repo

```bash
borescope memo                       # project decisions, danger zones, who worked where
borescope memo --who src/<area>      # find who has context on a specific area
borescope hotspots --top 15          # which production files are actively risky?
borescope smells                     # structural + concurrency antipattern report
borescope map --weight hotspot -o tui   # navigate the whole repo interactively
```

---

## Scenario: before touching a function

```bash
borescope callers <file>:<symbol> --depth 3 -o json   # everything that depends on this
borescope explain <file>:<symbol>                      # risk verdict, fanin, patterns, co-change
```

`confidence ≥ 0.7` callers are certain contracts. Below that are linker guesses — verify before assuming.

---

## Scenario: understanding what a function reaches

```bash
borescope paths <file>:<symbol> --depth 4 -o json     # full forward slice
borescope paths <entry> --to <target> --weight hotspot # point-to-point with load scoring
```

Open source only for frames in the tree you will actually edit.

---

## Scenario: reviewing a PR

```bash
borescope explain-pr <branch>                  # risk verdict, blast radius, co-change warnings
borescope explain-pr <branch> -o json          # structured for scripting
borescope diff main HEAD --weight hotspot       # call-tree diff with hotspot scores
```

`explain-pr` flags symbols that are hot + complex, files missing from the PR that historically co-change, and concurrency patterns in touched code.

---

## Scenario: finding load and concurrency risk in a call path

```bash
borescope paths <entry> --to <target> --weight hotspot --analyze
```

`--analyze` appends a `signals[]` array. Each entry has `kind`, `severity`, `detail` — cite `detail` directly.

Signal kinds: `lock_await`, `blocking_async`, `async_handoff`, `unbounded_loop`, `hot_symbol`, `high_complexity`, `path_depth`, `cross_file_boundary`, `external_boundary`.

```bash
borescope explain <target>     # follow up: fanin, patterns, co-change partners in plain English
```

---

## Scenario: after making a change

```bash
borescope diff -o json          # structured diff — changed/added/removed frames
borescope diff                  # tree view for PR description
borescope smells                # verify no new antipatterns introduced
borescope memo --update         # refresh worklog so teammates see what changed
```

---

## Interpreting output

- `┄0.4` — dashed confidence tag: linker guess, not a certain call. Verify before relying on it.
- `(ext)` node — callee outside the index (stdlib, OS, unindexed dep). Traversal stops here.
- `cochange` partners — files that historically move with the target. Touch or note them.
- `signals[].detail` — full sentence, composable. Cite it directly in explanations.
- Exit 3: ambiguous target — JSON candidates on stderr; pick the qualified name and retry.
- Exit 4: no index — run `borescope index` first.

---

## Output formats

| Format | Use for |
|---|---|
| `-o json` | agent consumption, scripting, piping |
| `-o tui` | interactive navigation (keyboard: j/k move, / filter, q quit) |
| `-o mermaid` | paste into GitHub PR, Claude Code, Cursor, any Markdown surface |
| `-o dot` | large graphs — pipe to `dot -Tpng` for image export |
| `-o folded` | flamegraph input — pipe to `inferno-flamegraph` |

```bash
borescope paths src/auth.rs:verify -o mermaid          # render in PR comment
borescope map -o dot --no-fence | dot -Tpng -o map.png # export large graph
```

---

## Token efficiency

- `borescope memo` → ~200 tokens — read before asking what the team already answered.
- `borescope explain <symbol>` → ~300 tokens — complexity, fanin, hotspot, patterns, co-change.
- `borescope paths -o json` → structured slice — open only the files you will edit.
- `borescope callers -o json` → caller list — skip files not in the result.
