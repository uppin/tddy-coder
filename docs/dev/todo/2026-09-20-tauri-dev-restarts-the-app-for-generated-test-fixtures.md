# 2026-09-20 — `tauri dev` restarts the running app for generated test fixtures

**Category:** Defect
**Source:** sandboxed-codebase-managed-workflow changeset, 2026-09-20

`./desktop-dev` restarts the whole application — tearing down the hosted `tddy-daemon`, every open
session, every RPC connection, and orphaning the session's jail — when a **generated test fixture**
is written inside a watched crate:

```
Info File …/packages/tddy-tool-engine/tests/fixtures/buildbox-root/home/dev/repo/
     .worktrees/sess/hello.txt changed. Rebuilding application...
```

Observed repeatedly on 2026-09-20 while testing a session by hand; each occurrence cost the session
being driven.

## Why `.taurignore` does not fix it — attempted and failed

`packages/tddy-desktop/src-tauri/.taurignore` was added in `d4ef5a69` with `**/tests/fixtures/`.
**It does not work, and the PR description claiming it did was wrong.**

`tauri dev` watches **66 separate roots** — `src-tauri` plus every workspace crate the binary
depends on, each announced individually:

```
Info Watching …/packages/tddy-desktop/src-tauri for changes...
Info Watching …/packages/tddy-core for changes...
Info Watching …/packages/tddy-actions for changes...
… 63 more
```

`.taurignore` sits beside `src-tauri/Cargo.toml` and its patterns resolve against **that** root.
`packages/tddy-tool-engine/tests/fixtures/…` is under a different root, so the file never applies
to it. The fix addressed the wrong scope.

Note the fixture tree is already gitignored (`packages/tddy-tool-engine/tests/fixtures/.gitignore`
lists `buildbox-root/`), and that does not help either — the watcher is not consulting git.

## What was not established

**Which process writes the file.** It was observed changing while no `cargo test` was running, and
the crate whose tests own it (`tddy-tool-engine`) was not under test at the time. Worth pinning down
before fixing, because it changes which of the options below is right.

## Options

| | Approach | Cost |
|---|---|---|
| A | A `.taurignore` per watched crate root | 66 files; matches how Tauri resolves them, and is horrible |
| B | Patterns relative to the workspace root in the existing file | Cheap to try; only works if Tauri resolves against the workspace, which the above suggests it does not |
| C | **Stop the tests writing into the source tree** — `tddy-tool-engine` builds a fake checkout under `tests/fixtures/buildbox-root/`; a `TempDir` ends the class | The real fix. Belongs to that crate, not to a desktop changeset |

C is the right one: a test writing into a watched source tree is the underlying fault, and the
watcher only makes it visible. A is a workaround that would rot as crates are added.

## Why it was not fixed in the originating PR

The originating change is about the sandboxed-codebase placement. C belongs to `tddy-tool-engine`
and is a behavioural change to someone else's test harness; A and B are workarounds that were tried
and did not hold. Filed rather than absorbed.

## Two more triggers, same watcher

**Atomic-write temp files.** An editor writing `foo.rs` atomically creates
`foo.rs.tmp.<pid>.<hash>` first, and the watcher rebuilds for that too:

```
Info File …/packages/tddy-tools/src/server.rs.tmp.64259.25ba69ff0ef0 changed. Rebuilding application...
```

So one source edit costs **two** restarts — one for the temp file, one for the real one. A
`*.tmp.*` exclusion would halve the churn even before the fixture question is settled, and unlike
the fixture patterns it would sit in a root `.taurignore` can actually reach for `src-tauri` itself.

**Operational consequence, which is the part that bites.** Background test runs and hand-testing
the running app **cannot share this worktree**. Any agent or developer running the suite restarts
the app the other is driving, dropping its session and orphaning its jail. Observed four times in
fifteen minutes on 2026-09-20 while one background implementer ran its verification loop and the
app was being tested by hand. Until this is fixed the two activities have to be serialised by
hand, which is a coordination cost nobody is told about.
