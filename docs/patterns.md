# Borescope — Semantic Pattern Detection

Borescope captures structural patterns inside every symbol during indexing using tree-sitter query packs. These patterns are language-agnostic: the same `.scm` rule fires for a Go mutex lock and a Rust `Mutex::lock()`. Cross-signalling these patterns with git signals surfaces real risk that neither static analysis nor git history alone can see.

---

## Captured patterns

| Pattern | What it means | Example triggers |
|---|---|---|
| `lock` | Mutex / RwLock acquisition | `mutex.Lock()`, `.lock().unwrap()`, `synchronized` |
| `await` | Async yield point | `.await`, `await expr` |
| `block_on` | Blocking call inside async context | `block_on(...)`, `run_until_complete(...)` |
| `spawn` | Thread / goroutine / task creation | `go func()`, `tokio::spawn`, `thread::spawn` |
| `loop` | Any loop construct | `for`, `while`, `loop {}` |
| `alloc` | Allocating method calls | `.clone()`, `.collect()`, `.to_string()`, `new(...)` |
| `chan` | Channel send / receive | `ch <- x`, `<-ch`, `tx.send(...)` |
| `timer` | Timer / ticker creation | `setTimeout`, `time.NewTicker` |

Patterns are stored per-symbol in the index. Query them with `-o json`:
```bash
borescope explain src/worker.rs:dispatch -o json | jq '.patterns'
```

---

## Built-in smell detectors

`borescope smells` cross-signals captured patterns with git metrics:

| Detector | Signal combination | Risk |
|---|---|---|
| `lock_across_await` | `lock` + `await` in same fn | Deadlock under async runtimes |
| `sync_in_async` | `block_on` in any fn | Starves executor thread pool |
| `alloc_in_hotspot` | 3+ `alloc` captures + hotspot > 0.7 | GC pressure in hot path |
| `high_complexity_bottleneck` | complexity > threshold + fanin > threshold + hotspot > 0.5 | Hardest symbol to safely change |
| `spawn_in_tight_loop` | `spawn` + `loop` | Goroutine / thread explosion |
| `unbalanced_fanout` | fanout > 8 + fanin < 2 + churn < 3 | Probably dead infrastructure |

Thresholds for `alloc_in_hotspot` and `high_complexity_bottleneck` are configurable via `.borescope/thresholds.toml`. See [`docs/commands.md`](commands.md#configuration).

---

## Structural smells (git-based, no patterns needed)

| Detector | Definition |
|---|---|
| `shotgun-surgery` | One file with ≥ 4 strong (≥ 0.5) co-change partners |
| `god-file` | p95 LOC **and** p95 churn — big and changing constantly |
| `stale-core` | ≥ 730 days since last change + in the p90 churn bracket — important but neglected |
| `tangled-pair` | Two files with strength ≥ 0.8 **both ways** — should probably be one |

---

## Custom smell rules

Define your own pattern combinations in `.borescope/smells.toml`:

```toml
[[rules]]
name        = "dangerous_combo"
description = "holds lock while spawning inside a loop — goroutine leak + deadlock"
patterns    = ["lock", "spawn", "loop"]
severity    = "critical"

[[rules]]
name        = "async_allocation_loop"
description = "allocates inside a loop inside an async fn — latency spikes"
patterns    = ["alloc", "loop", "await"]
severity    = "high"
```

A rule fires when **all** listed patterns appear on the same symbol. The finding appears in the `semantic` section of `borescope smells` output with `kind = rule.name`.

---

## Query pack overrides

The built-in queries live in `crates/bs-extract/queries/<lang>.scm`. To add or change captures for a specific repo:

1. Create `.borescope/queries/<lang>.scm` at the repo root.
2. Re-run `borescope index`.

The repo-local file fully replaces the built-in query for that language. To extend (not replace), copy the built-in and add your captures.

---

## `--analyze` flag

`borescope paths <target> --analyze` emits a structured signal array instead of a tree:

```json
[
  { "kind": "complexity", "severity": "high",   "detail": "complexity=18, threshold=10" },
  { "kind": "lock_across_await", "severity": "critical", "detail": "holds lock across .await" },
  { "kind": "fanin", "severity": "medium", "detail": "called by 12 symbols — high blast radius" }
]
```

Designed for LLM consumption: machine-readable, plain-English `detail`, no tree traversal required.
