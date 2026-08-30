# Borescope Agent Contract — JSON Schema 1

All commands accept `-o json` and produce a response conforming to this schema.
**Stability guarantee**: additive changes only under `"schema": 1`. Unknown fields must be ignored.

## Root object

```json
{
  "borescope": "0.4.3",
  "schema": 1,
  "query": {
    "cmd": "paths|callers|diff|branch|map",
    "target": "<target string as passed on CLI>",
    "depth": 3,
    "weight": "none|loc|fanin|churn|hotspot|diff"
  },
  "root": <TreeNode | null>,
  "unresolved_edges": 0,
  "cochange": [<CochangeEntry>],
  "truncated": {
    "depth": false,
    "nodes_omitted": 0
  },
  "unresolved": []
}
```

- `unresolved_edges`: count of call edges in this response that could not be resolved to a known symbol (shown as `external: true` leaf nodes in the tree).

## TreeNode

```json
{
  "id": "fac797185f8b7e72",
  "name": "HandleCheckout",
  "qualified": "api/checkout.go:HandleCheckout",
  "file": "api/checkout.go",
  "span": [41, 88],
  "weight": 0.82,
  "confidence": 1.0,
  "mark": null,
  "external": false,
  "children": [<TreeNode>]
}
```

- `weight`: normalized 0..1 within this response (max-scaled). 0.0 when `--weight none`.
- `confidence`: 0..1. Edge is certain if ≥ 0.9; uncertain if < 0.7. Rendered as dashed connector in tree output.
- `mark`: `"+"` added, `"-"` removed, `"~"` modified, `null` context frame. Only set for `diff`/`branch` commands.
- `external`: `true` when the callee is unresolvable (stdlib, OS, unindexed dep). Name is the raw callee identifier; no `id` or `file`. Confidence is 0.0. Rendered as `(ext)` in tree output.
- `children`: recursive TreeNode array. Empty at max depth; parent's `truncated.depth` is true.

## CochangeEntry

```json
{
  "file": "internal/auth/middleware.go",
  "strength": 0.85,
  "support": 17
}
```

- `strength`: P(file|target) — probability this file changes given the target changed.
- `support`: raw commit count where both changed together.

## Confidence rubric

| Situation | Confidence |
|---|---|
| Same-file resolution | 1.0 |
| Import-qualified, unique target | 0.9 |
| Unique name match within imported modules | 0.7 |
| Method name match on candidate types (multi-target) | 0.5 per edge |
| Global name match, multiple candidates | min(0.3, 1/N) per edge |
| Unresolvable (dynamic dispatch, metaprogramming) | 0.1 |

## Exit codes

```
0  success
1  runtime error
2  usage error
3  ambiguous target — candidates listed on stderr as JSON array of qualified names
4  index missing or corrupt — run `borescope index` and retry
5  grammar unavailable for the requested file extension
```

## Consuming exit 3

When exit code is 3, stderr contains a JSON array:
```json
["internal/auth.Token.Verify", "pkg/mock.Token.Verify"]
```
Pick the correct candidate and retry with the fully-qualified name.

## Schema evolution

- New fields may be added at any time without a schema bump.
- Fields will never be removed or renamed under schema 1.
- A breaking change increments `"schema"` to 2; schema 1 responses will still have `"schema": 1`.

---

## SQLite database schema (power-user API)

The index lives at `.borescope/index.db`. It is a valid SQLite 3 database you can query directly with any SQLite client. The schema below is stable across patch versions; columns may be added but existing ones are never renamed or dropped.

```sql
-- Source files in the repo
files(
  id            INTEGER PRIMARY KEY,
  path          TEXT NOT NULL UNIQUE,   -- repo-root-relative path, forward slashes
  lang          TEXT NOT NULL,          -- "rust" | "go" | "python" | "typescript" | ...
  loc           INTEGER NOT NULL,       -- line count
  file_hash     TEXT NOT NULL DEFAULT '' -- mtime:size fingerprint for incremental indexing
)

-- Git-derived metrics, one row per file
git_stats(
  file_id       INTEGER PRIMARY KEY REFERENCES files(id),
  churn         INTEGER NOT NULL,        -- commit count touching this file
  age_days      INTEGER NOT NULL,        -- days since last commit
  last_commit_sha TEXT,
  last_commit_ts  INTEGER,               -- Unix timestamp of last commit
  hotspot       REAL NOT NULL            -- churn × exp(-λ × age_days), λ≈0.003
)

-- Co-change pairs (files that commit together)
cochange(
  file_a_id     INTEGER NOT NULL REFERENCES files(id),
  file_b_id     INTEGER NOT NULL REFERENCES files(id),
  support       INTEGER NOT NULL,        -- commits where both changed
  strength      REAL NOT NULL,           -- P(b|a) — Jaccard-style
  strength_rev  REAL NOT NULL,           -- P(a|b)
  PRIMARY KEY (file_a_id, file_b_id),
  CHECK (file_a_id < file_b_id)         -- canonical ordering, lo < hi
)

-- Symbols (functions, methods, types)
symbols(
  id            TEXT PRIMARY KEY,        -- stable hex hash of (file, name, kind)
  kind          TEXT NOT NULL,           -- "function" | "method" | "type"
  name          TEXT NOT NULL,           -- short name
  qualified     TEXT NOT NULL,           -- "path/to/file.rs:FuncName"
  file_id       INTEGER NOT NULL REFERENCES files(id),
  span_start    INTEGER NOT NULL,        -- start line (1-based)
  span_end      INTEGER NOT NULL,        -- end line (inclusive)
  lang          TEXT NOT NULL,
  churn         INTEGER NOT NULL,
  age_days      INTEGER NOT NULL,
  loc           INTEGER NOT NULL,
  complexity    INTEGER NOT NULL,        -- cyclomatic complexity
  hotspot       REAL NOT NULL,
  patterns      TEXT NOT NULL DEFAULT '' -- JSON array, e.g. ["lock","await"]
)

-- Call graph edges
edges(
  from_id       TEXT NOT NULL,           -- symbol id, or "file:<path>", or "unresolved:<name>"
  to_id         TEXT NOT NULL,           -- symbol id, or "external:<name>", or "import:<name>"
  kind          TEXT NOT NULL,           -- "calls" | "contains" | "imports" | "reference"
  confidence    REAL NOT NULL,           -- 0.0..1.0; see confidence rubric above
  meta          TEXT,                    -- reserved JSON blob
  PRIMARY KEY (from_id, to_id, kind)
)
-- kind values:
--   "calls"     — direct call expression (foo(), self.bar(), Mod::baz())
--   "contains"  — file contains symbol (file:<path> → symbol id)
--   "imports"   — use/import declaration (file → import:<name>)
--   "reference" — function passed as a value (callback, higher-order arg, spawn argument)
--                 confidence ≤ 0.5; lower than calls because static analysis cannot confirm
--                 the function is ever actually invoked

-- Bookkeeping
meta(
  key           TEXT PRIMARY KEY,
  value         TEXT NOT NULL            -- includes "schema_version"
)
```

### Useful queries

```sql
-- Fan-in: symbols with the most callers
SELECT s.qualified, COUNT(*) AS fanin
FROM edges e JOIN symbols s ON s.id = e.to_id
WHERE e.kind = 'calls' AND e.confidence >= 0.7
GROUP BY e.to_id ORDER BY fanin DESC LIMIT 20;

-- Hottest production files (excluding test paths)
SELECT f.path, g.hotspot, g.churn, g.age_days
FROM files f JOIN git_stats g ON g.file_id = f.id
WHERE f.path NOT LIKE '%/test%' AND f.path NOT LIKE '%_test.rs'
ORDER BY g.hotspot DESC LIMIT 20;

-- Symbols with lock+await (deadlock candidates)
SELECT s.qualified, s.hotspot
FROM symbols s
WHERE s.patterns LIKE '%"lock"%' AND s.patterns LIKE '%"await"%'
ORDER BY s.hotspot DESC;

-- Folded stacks for a call tree rooted at a symbol (feed to inferno-flamegraph)
-- Use borescope paths <sym> -o folded instead; this is the manual equivalent.
WITH RECURSIVE tree(id, path, weight) AS (
  SELECT id, name, hotspot FROM symbols WHERE qualified = 'src/http/router.rs:dispatch'
  UNION ALL
  SELECT s.id, tree.path || ';' || s.name, s.hotspot
  FROM tree JOIN edges e ON e.from_id = tree.id
            JOIN symbols s ON s.id = e.to_id
  WHERE e.kind = 'calls' AND e.confidence >= 0.7
)
SELECT path, MAX(1, CAST(weight * 1000 AS INTEGER)) FROM tree
WHERE id NOT IN (SELECT from_id FROM edges WHERE kind='calls');
```

### `edges.meta` field

Currently unused by the CLI. Reserved for future structured metadata per edge (e.g. call-site line number, argument count). Treat as opaque JSON or NULL.
