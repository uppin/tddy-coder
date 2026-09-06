# 2026-07-30 — `cursor_cli_peer_spawn_records_the_orchestrator_link_even_without_repo_path` fails on `master` — resolved 2026-08-13

**Category:** Known failing test
**Status:** Resolved
**Source:** session-attachment-start-materialization wrap, 2026-07-30

- `packages/tddy-daemon/tests/cursor_cli_session_acceptance.rs` — the test (introduced by #358)
  starts a cursor-cli session whose `stack_parent` names an orchestrator session it never creates on
  disk, and expects `StartSession` to succeed. It fails with
  `FailedPrecondition: could not resolve stack parent branch: session file missing: parent session
  not found under sessions tree`, i.e. `tddy_core::session_chain::resolve_chain_base_ref` requires the
  parent session file that `unified_chain_base_resolution.rs` separately pins as a hard requirement.
- Either the test must write the parent session's metadata, or spawn-time resolution must tolerate a
  `stack_parent` with no session file (recording `orchestrator_session_id` while falling back to the
  default base) — a behaviour decision, not a test fix.
- Not a regression from any in-flight branch: reproduced with every file on the code path
  byte-identical to `master`. #367 (which last touched the test file) reported no CI checks.
- **Resolved** by the deterministic-test-suite changeset (#385): the fixture was what was wrong.
  `an_orchestrator_session(...)` now writes a real parent changeset, because production correctly
  refuses an unresolvable `stack_parent` with `FailedPrecondition` rather than silently basing the
  child off the default branch — the no-fallback behaviour is the one worth keeping.
