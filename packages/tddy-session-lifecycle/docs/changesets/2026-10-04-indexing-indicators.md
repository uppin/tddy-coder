# 2026-10-04 — Session start phases and code-index warm-up progress

**Type:** Feature

`#live-plan` 12/15 — PR [#571](https://github.com/uppin/tddy-coder/pull/571),
`feature/live-plan/indexing-indicators`. Cross-package entry:
[2026-10-04-indexing-indicators.md](../../../../docs/dev/changesets/2026-10-04-indexing-indicators.md). Product entry:
[2026-10-04-indexing-indicators.md](../../../../docs/ft/web/changelog/2026-10-04-indexing-indicators.md).

The claude-cli, cursor-cli and workspace starts report `StartPhase` events through
`AttachmentProgressSink::begin_phase` / `end_phase`; the cursor-cli start is
`spawn_cursor_cli_session_reporting`, with `spawn_cursor_cli_session_inner` a wrapper for a caller with
nobody watching. `stream_start_session_at_session_coordinate` sends the terminal event only after the
progress-forwarding task has drained, so a result never overtakes a phase's end, and logs a forwarding
task that ended abnormally. The new port `SessionWorktreeObserver`
(`connection_service/session_worktree_observer.rs`) is installed with
`DaemonSessionHost::with_worktree_observer`; `announce_worktree_ready` calls it once per successfully
started claude-cli, cursor-cli or workspace session. Docs: [session-service.md](../session-service.md),
[module-layout.md](../module-layout.md).

**Not covered.** The sandboxed claude-cli and cursor-cli starts, the tool start and the split start report
no phases and announce no worktree; children spawned by a PR-stack orchestrator or a grill-me conversation
discard their progress. Deferred with the developer's consent: backlog entry
`2026-10-03-start-phases-and-code-index-warm-skip-sandboxed-tool-and-split-starts` (open).

**Tests (scoped).** `tests/start_phase_acceptance.rs` 2 passed: the worktree and agent phases precede the
result, and the semantic-index phase is streamed when enabled (the start then fails without an embedder,
sending no END). `./test -p tddy-session-lifecycle` also shows the 22 environmental failures
(sandboxed-session suites and `session_sync_livekit_acceptance`) listed in
[test-suites.md](../test-suites.md#what-a-local-run-shows); they were not compared with a clean base.

**File length (deferred).** `src/cursor_cli_spawn.rs` grew 445 → 532 production lines — this change
crossed the 500 budget; the code-issue record `complexity-cursor-cli-spawn-spawn-cursor-cli-session-inner`
is regressed, and backlog entry `2026-10-04-cursor-cli-spawn-crossed-the-file-budget-in-indexing-indicators`
records the deferral (developer's consent). `complexity-svc-start-session-core-start-session-core`:
`start_session_core` 358 → 373 lines (+15), record regressed, the split stays with the engine-refused
early-return guards.

**Clean code.** The functions this change wrote are within limits. Touched pre-existing long functions
(`start_session_core`, `spawn_claude_cli_session_inner`, `spawn_cursor_cli_session_reporting`,
`stream_start_session_at_session_coordinate`, `cursor_cli_semantic_env` — now seven parameters) were
extended, none written over a limit by this change; a split would collide with PR #572 and the parents.
