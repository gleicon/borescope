# Borescope — Output Formats

Select with `-o <format>` (global flag, works on all query commands).

---

## tree (default)

ANSI call tree with weight bars and confidence annotations.

```
HandleCheckout                           ██████ 0.82
├─ validateCart()                        ████   0.61
│  ├─ PriceService.quote()               ▌      0.07
│  └─ Inventory.check()                  █      0.12
├─ services.boot()                       ▌      0.05
└─ SessionManager.open()   ┄┄ 0.5        █      0.15
```

- `██████ 0.82` — weight bar (normalized 0..1 in this response) + score
- `┄┄ 0.5` — confidence annotation; edge is a linker guess, not certain
- `(ext)` — unresolvable callee (stdlib, OS, unindexed dependency)
- `+` / `-` / `~` prefix — diff/branch frame classification

Add `--no-color` for plain ASCII output (CI, log files).

---

## tui

Interactive terminal UI. Best for exploration.

```bash
borescope map --weight hotspot -o tui
borescope paths src/http/router.rs:handle --depth 5 -o tui
```

![borescope paths -o tui](../docs/screenshots/borescope-paths.gif)

**Detail panel** (cyan bar, always visible): shows what the current weight mode means and the selected node's exact score + file path.

### Keybindings

| Key | Action |
|---|---|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `Enter` / `Space` | Expand / collapse node |
| `g` | Jump to top |
| `G` | Jump to bottom |
| `/` | Enter filter mode (by name or file substring) |
| `Esc` | Exit filter / clear |
| `q` | Quit |

---

## json

Stable schema 1 contract — designed for agent and tool consumption.

```bash
borescope paths src/auth.rs:verify -o json
borescope callers src/auth.rs:verify -o json | jq '.root.children | length'
borescope smells -o json | jq '.semantic | group_by(.kind)'
```

**Stability guarantee**: additive changes only under `"schema": 1`. New fields may appear; existing fields are never removed or renamed. Breaking changes bump the schema number.

Full schema documentation: [`docs/agent-contract.md`](agent-contract.md).

---

## folded

Brendan Gregg collapsed format — pipe to `inferno-flamegraph` to produce SVG.

```bash
borescope paths src/http/router.rs:dispatch -o folded \
  | inferno-flamegraph > flame.svg
open flame.svg
```

Install inferno: `cargo install inferno`.

---

## html

Self-contained collapsible call tree. No network requests; single file you can open or attach to a PR.

```bash
borescope paths src/auth.rs:verify -o html > auth_verify.html
open auth_verify.html
```

---

## Weight modes (`--weight`)

Affects the bar chart score on every frame. All weights are normalized 0..1 within a single response.

| Weight | Score |
|---|---|
| `none` (default) | 0.0 — bars disabled |
| `loc` | Lines of code (raw size) |
| `fanin` | Number of callers (how central) |
| `churn` | Commit frequency (how often it changes) |
| `hotspot` | Churn × recency decay (active fire risk) |
| `diff` | Change size in current diff (only with `diff`/`branch`) |

`--weight diff` is only valid with the `diff` or `branch` command; all others exit 2 with a clear error.
