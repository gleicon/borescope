# Borescope — Performance & Storage

## Performance targets (release build, 8-core)

| Operation | Target |
|---|---|
| Cold `index --no-git`, 300k LOC | < 30 s |
| `index --git`, 10k commits (added to above) | + < 20 s |
| Incremental index, 10 changed files | < 1 s |
| `paths` / `callers` at depth 4 | < 200 ms |
| `diff` on 50-file PR | < 2 s |
| Release binary size (all Tier 1+2 grammars) | < 60 MB |

Measured on an 8-core machine with a 300k-LOC polyglot repository in release mode (`cargo build --release`).

---

## Incremental indexing

Borescope fingerprints each file by mtime + size at index time. On subsequent runs, unchanged files are skipped entirely — only modified files are re-parsed and re-linked. This makes `borescope index` safe to re-run before any query.

The two-phase pattern keeps interactive latency low:

```bash
borescope index --no-git    # Phase 1: parse + link only (~seconds for any size repo)
borescope index --git &     # Phase 2: git history mining in background
```

`paths`, `callers`, `map`, and `explain` work as soon as Phase 1 completes.  
`hotspots`, `coupled`, `smells`, and `age` need Phase 2 — they return empty results until git mining finishes.

---

## Storage

**Location**: `.borescope/index.db` — SQLite with WAL mode.  
**Auto-excluded**: added to `.gitignore` automatically on first index.  
**Recovery**: delete `.borescope/` and re-run `index` to rebuild from scratch.  
**Schema migration**: additive only (new columns via `ALTER TABLE`). No manual migration steps needed when upgrading.

### Disk usage (approximate)

| Repo size | Index size |
|---|---|
| 10k LOC | < 5 MB |
| 100k LOC | < 30 MB |
| 1M LOC | < 200 MB |

Actual size depends on symbol density and git history depth.

---

## Memory

Peak RSS during a 1M-LOC cold index stays under 2 GB. Parsing is parallelised across CPU cores (rayon); the DB write phase is serial. For very large repos, Phase 1 memory is dominated by the rayon thread pool holding parsed ASTs before the write phase drains them.

---

## Build flags

```bash
cargo build --release          # optimised binary, strip enabled
cargo build                    # debug build — slower but easier to trace
```

The `[profile.release]` in `Cargo.toml` sets `opt-level = 3` and `strip = true`. The release binary is statically linked with bundled SQLite.
