# Borescope Agent Contract — JSON Schema 1

All commands accept `-o json` and produce a response conforming to this schema.
**Stability guarantee**: additive changes only under `"schema": 1`. Unknown fields must be ignored.

## Root object

```json
{
  "borescope": "0.4.1",
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
| Global name match, multiple candidates | 0.3 per edge |
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
