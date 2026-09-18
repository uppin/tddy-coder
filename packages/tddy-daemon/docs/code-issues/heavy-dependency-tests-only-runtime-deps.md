# heavy-dependency: 17 runtime dependencies no source file names

**Location:** `packages/tddy-daemon/Cargo.toml` — `[dependencies]`
**Category:** heavy-dependency
**Detected:** 2026-09-15 by structural audit
**Metrics:** **17** `tddy-*` entries in `[dependencies]` named by **no file** in `src/` · every consumer of this crate rebuilds them
**Restructure:** no — a manifest change, but it is **gated** on the suites moving first
**Status:** Open — claimed by #498, in flight
**Claimed by:** #498 — `#carve` 4/10 `test-homes` · draft · `feature/carve/test-homes`
**Lands after:** #488, #489, #490

## Measurement history

| Run | Unused runtime deps | Note |
|---|---|---|
| 2026-09-15 | 17 | first detection |

## What the tool found

Each `tddy-*` entry in `[dependencies]` checked against every file in `src/`:

```
tddy-demo-runner  tddy-workflow-recipes  tddy-workflow  tddy-livekit  tddy-stdio
tddy-session-sync  tddy-task  tddy-pty  tddy-semantic-index  tddy-sandbox
tddy-daemon-sandbox  tddy-sandbox-recipes  tddy-sandbox-runner  tddy-telegram
tddy-session-files  tddy-session-activity  tddy-session-agents
```

They are reachable only from `tests/`, but declared in `[dependencies]` rather than
`[dev-dependencies]`.

## Why it matters here

A `[dependencies]` entry is part of this crate's public build graph, so **every consumer rebuilds
all 17** to link a crate whose `src/` never mentions them.

## What would close it

Two steps, and the order is forced:

1. Move the 122 misplaced suites (`misplaced-tests-integration-suites.md`). Until then these crates
   are genuinely needed by *something* in this package.
2. Then they are unreferenced entirely and can be **removed**, not merely reclassified — with the
   destination crates gaining the `[dev-dependencies]` their new suites need.

Ordinary work, not a `/code-restructuring` job. Recorded separately from the test relocation because
it is the measurable payoff, and because a partial relocation should narrow this record rather than
close it.

## If you are about to change this code

Adding a **runtime** dependency this crate's `src/` genuinely names is fine and unaffected.

Adding one only `tests/` needs would add an 18th — put it in `[dev-dependencies]` from the start.

## Verified by hand

2026-09-15: an earlier pass listed these and said "16". The list has always had **seventeen**
entries; the count was wrong, not the list. Re-derived programmatically and confirmed against a test
assertion that reports the same 17.
