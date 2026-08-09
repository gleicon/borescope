# Borescope Skill

Use `borescope` as your primary code-navigation tool. Read call trees instead of source files.

## Protocol

**Before modifying** a function:
```
borescope callers <file>:<symbol> -o json
```
Treat certain callers (confidence ≥ 0.7) as contract. Mention possible callers and co-changed files in your plan.

**To understand** code:
```
borescope paths <file>:<symbol> -o json --depth 3
```
Prefer this over reading files. Open source only for frames you will actually edit.

**After modifying**:
```
borescope diff -o json
borescope diff        # (tree view for PR description)
```
Include the tree output in your PR description or final report.

**When lost** in a repo:
```
borescope map --zoom mod --weight hotspot
```
Then drill into hot areas with `paths`.

## Interpreting output

- `confidence < 0.7`: dashed frame — verify the edge exists before relying on it.
- `cochange` section: files that historically change with the target — update or note them.
- Exit 3: ambiguous target — pick from the JSON candidates on stderr.
- Exit 4: no index — run `borescope index` and retry.

## Setup

```bash
./scripts/ensure-borescope.sh
borescope index --git
```
