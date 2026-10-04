# 2026-10-04 — `restructure` anchors and snapshot friction seen in `#carve` 16a

**Category:** Future enhancement (engine ergonomics)
**Source:** `#carve` 16/21, [#531](https://github.com/uppin/tddy-coder/pull/531), changeset
[`2026-09-26-carve-lifecycle-ports`](../1-WIP/2026-09-26-carve-lifecycle-ports.md)

Three small problems met while running the engine against the warm index daemon. None blocked a plan;
each cost time.

## 1. A package-relative path is refused with a message that does not say what to change

`restructure anchors src/split_session.rs --items …` (a path relative to the package) fails with:

```
the index daemon refused this run (InvalidArgument): plan is malformed:
src/split_session.rs is in no package: no Cargo.toml declaring a [package] sits above it
```

The path has to be written from the **repo root** (`packages/tddy-session-lifecycle/src/split_session.rs`).
**Should:** when the path does not resolve but `<repo root>/<path>` under exactly one package does, say
"write it from the repo root: `packages/<pkg>/<path>`", or resolve it.

## 2. The first request after the daemon starts took over five minutes

`./run-index-daemon` reports ready, then the first `anchors` call waits for the index (observed: more than
five minutes on this workspace); later calls took seconds. **Should:** `./run-index-daemon` (or the
daemon's ready signal) cover the warm-up, or `--status` report "indexing" versus "ready", so the first
real request does not pay for it.

## 3. `restructure snapshot` exited with `lsp server exited` on a v2 header

Run on a plan whose header was `{"v":2,"files":{}}`: `lsp server exited`. **Cause not established**:
the empty header may be the trigger, and a v2 header never refuses a run, so it was not needed. Needs a
minimal reproduction (empty `files`, one op) before anything is concluded.

## Hand edit?

None. These are all engine-side.
