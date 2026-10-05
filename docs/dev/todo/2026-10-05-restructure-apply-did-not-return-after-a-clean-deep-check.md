# 2026-10-05 — `restructure apply` of a one-field `rename_symbol` did not return, though `check --deep` had answered

**Category:** Known defect (engine; the edit was done by hand)
**Source:** #carve 17/21 (PR #532) stage B2

## What I ran

A single `rename_symbol` on an item anchor (a struct field, `ClonesClaimedOnOwningPeers::connection`
-> `roster`, `svc_session_agent_port_adapters.rs`) against the warm index daemon, after editing seven
other files of the same crate in the working tree:

```
restructure check <plan> --deep      -> "no findings"            (finished)
restructure apply <plan>             -> did not return
```

The apply sat at 0% CPU for over 10 minutes (killed), and a second attempt under `timeout 150` ended
at the timeout. The last lines of its output:

```
indexing (+0ms): rename: `roster` — collecting edits from rust-analyzer
indexing (+0ms): waiting for type inference at the anchor
indexing (+2.0s): working: build script num-bigint run
```

The daemon (`tddy-index-daemon`, pid 98405) stayed up and answered `--status`; its log's last line was
a `warm ... answered (+7m23s)` from the first request of the session. An earlier apply of four
`rename_symbol` ops in the same session, on a tree with fewer edits, returned in seconds.

## What I did by hand

Renamed the field, its one constructor site and the four `self.connection` uses; `cargo check` closed it.

## What capability would remove it

`apply` should report progress against a deadline, or fail with the stage it is waiting on, when the
daemon's index is stale against the working tree; a plan whose `check --deep` has answered should not
be able to wait indefinitely for a "type inference" signal at the anchor. I did not investigate the
cause.

## Minimal reproduction

Not reduced. Warm the index, edit several files in the crate, then `apply` a one-op `rename_symbol`
plan whose header lists only the anchored file.
