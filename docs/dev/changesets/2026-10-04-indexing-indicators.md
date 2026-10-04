# 2026-10-04 — Session start phases and code-index warm-up progress

**Type:** Feature

`#live-plan` 12/15 — PR [#571](https://github.com/uppin/tddy-coder/pull/571),
`feature/live-plan/indexing-indicators`. Product entry:
[2026-10-04-indexing-indicators.md](../../ft/web/changelog/2026-10-04-indexing-indicators.md). The next
node, the plan dialog, is PR [#572](https://github.com/uppin/tddy-coder/pull/572).

| Package | Entry |
|---|---|
| `tddy-service` | [indexing-indicators](../../../packages/tddy-service/docs/changesets/2026-10-04-indexing-indicators.md) |
| `tddy-session-lifecycle` | [indexing-indicators](../../../packages/tddy-session-lifecycle/docs/changesets/2026-10-04-indexing-indicators.md) |
| `tddy-daemon-rpc` | [indexing-indicators](../../../packages/tddy-daemon-rpc/docs/changesets/2026-10-04-indexing-indicators.md) |
| `tddy-daemon` | [indexing-indicators](../../../packages/tddy-daemon/docs/changesets/2026-10-04-indexing-indicators.md) |
| `tddy-worktree-service` | [indexing-indicators](../../../packages/tddy-worktree-service/docs/changesets/2026-10-04-indexing-indicators.md) |
| `tddy-session-files` | [indexing-indicators](../../../packages/tddy-session-files/docs/changesets/2026-10-04-indexing-indicators.md) |
| `tddy-web` | [indexing-indicators](../../../packages/tddy-web/docs/changesets/2026-10-04-indexing-indicators.md) |

`tddy-connectrpc-testkit` gains `registerServerStreamFallback`; the package has no `docs/` directory, so
the `tddy-web` entry carries it.

**What it does.** `StreamStartSession` reports the start's steps (worktree, semantic index, agent) as
`StartPhase` events, and the web streams every start so the create pane names the current step. Once a
started session's worktree exists, a daemon with `index_daemon:` warms its code index in the background
through a new port, `SessionWorktreeObserver`, which `tddy-session-lifecycle` defines and the daemon's
`IndexWarmupObserver` (in `tddy-daemon-rpc`) implements. `code_navigation.WatchCodeIndex` streams the
progress, and the session header shows `Indexing — <phase> <n>%` until ready. Starting a session never
waits on indexing, and a failed warm shows its reason without affecting the session.

**Decisions.**

- Phases ride the existing `AttachmentProgressSink` rather than a second sink: it already reaches every
  session type's start, and unary `StartSession` keeps discarding everything.
- `CodeIndexProgress` is `IndexProgress` plus `error`, declared in `code_navigation.proto`: the web reads
  only `tddy-service`'s protos.
- The warm-up module lives in `tddy-daemon-rpc`, beside the navigation service that reads its progress,
  so `tddy-daemon/src` stays wiring only (`unbundle_endpoint`); `index_daemon/` is untouched.
- `WatchCodeIndex` is authorised by session ownership (`resolve_owned_session_dir`), not by the token
  alone; a foreign session and a missing one answer the same `NotFound`, and `follow` never creates an
  entry for an id nothing warmed.
- Every start streams (developer decision); the unary `startSession` stubs of existing component specs
  keep working through `registerServerStreamFallback`.

**Not covered.** The sandboxed claude-cli and cursor-cli starts, the tool and split starts, and children
spawned by a PR-stack orchestrator or a grill-me conversation report no phases and trigger no warm; they
load their index on their first navigation request.

**Tests (scoped).** `tddy-session-lifecycle` `start_phase_acceptance` 2 passed; `tddy-daemon`
`code_index_warmup_acceptance` 6, `code_navigation_acceptance` 7, `unbundle_endpoint` 4,
`test_placement` 4 passed; `tddy-lsp-executor` suites green; `tddy-web` `ServerStreamFallback.cy` 7 and
`CreateSessionPane.cy` 29 passed. CI run 37182363649 on `0e7e07e2`: Rust lint (workspace `cargo fmt
--check` and `cargo clippy -D warnings`), Rust build, arm64 build, Generated code and both Web test jobs
passed; its Rust test job was cancelled (the parent merged and the base moved), so **no whole-workspace
Rust test result exists for this head** — an earlier run on `f4e5929f` failed only `unbundle_endpoint`
(since fixed by the parent) and the create-pane spec (since fixed here). Local failures in
`tddy-session-lifecycle` (22, environmental) and `tddy-worktree-service` (2) were not compared with a
clean base; neither appeared in that earlier CI run.

**Code issues and file length (all deferred with the developer's consent; the stack rule — dependent
PR #572 and the parents touch these files — keeps the splits out of the stack).**

| File | Production lines | Record |
|---|---|---|
| `tddy-session-lifecycle/src/cursor_cli_spawn.rs` | 445 → 532 (crossed 500) | `complexity-cursor-cli-spawn-spawn-cursor-cli-session-inner`, regressed |
| `tddy-daemon/src/runtime.rs` | 1,650 → 1,668 | `oversized-file-runtime`, regressed; `complexity-runtime-build` 910 → 928 lines, regressed |
| `tddy-worktree-service/src/service.rs` | 752 → 777 | `oversized-file-service`, regressed |
| `tddy-web/.../CreateSessionPane.tsx` | 783 → 785 | `oversized-file-create-session-pane`, opened |
| `tddy-web/.../SessionMainPane.tsx` | 666 → 674 | `oversized-file-session-main-pane`, opened |

`complexity-svc-start-session-core-start-session-core`: `start_session_core` 358 → 373 lines, regressed.
Generated `buf` output (`session_pb.ts`) is excluded from the file-length measure by design.

**Clean code.** Every function this change wrote is within the limits (`warm_for_session` 36 lines,
`warm` 30, `resolve_owned_session_dir` 12; `watch_code_index` 37 lines at nesting 4). Pre-existing long
functions it touched were extended, none written over a limit by this change: `runtime.rs::build`,
`start_session_core`, `spawn_cursor_cli_session_reporting`, `spawn_claude_cli_session_inner`,
`stream_start_session_at_session_coordinate`, `cursor_cli_semantic_env` (now seven parameters),
`SessionHost::new`. The claude-cli and cursor-cli `announce_worktree_ready` blocks in
`svc_start_session_core.rs` repeat; a helper is a candidate and was left as is.

**Backlog.** No entry was resolved by this change. Added and open:
`2026-10-03-start-phases-and-code-index-warm-skip-sandboxed-tool-and-split-starts`,
`2026-10-04-cursor-cli-spawn-crossed-the-file-budget-in-indexing-indicators` and
`2026-10-04-restructure-move-cluster-to-crate-leaves-a-modules-directory-children-behind`. Consumed as a
reference, not resolved: `2026-09-16-warm-ready-means-a-live-server-not-a-loaded-graph` (the indicator
reads `Warm`'s `ready`, which waits for the graph).

**Not pinned.** No test drives a real session start through the observer into a warm end to end; the
hook is pinned at its two halves (`warm_for_session` directly, and the start's phases). No test pins an
END clearing the web's phase text or a stream error ending it.
