# Changeset: Cursor Agent CLI Session

**Date**: 2026-07-05
**Status**: ✅ Complete (Phase 1)
**Type**: Feature
**PRD**: [docs/ft/daemon/cursor-cli-session.md](../../ft/daemon/cursor-cli-session.md)

## Affected Packages

- `tddy-daemon` — `cli_session_manager`; `cursor-cli` Start/Resume; Telegram `/start-cursor`
- `tddy-core` — `cursor_hooks.rs`; unified hook event map; `cursor_cli_models()`
- `tddy-tools` — `session-hook` stdin mapping; `list-models` pseudo-agent `cursor-cli`
- `tddy-web` — third session type in `CreateSessionPane` + terminal mount helpers

## Summary

Add `session_type = "cursor-cli"` that spawns Cursor Agent CLI (`agent`) in a PTY inside a managed
worktree, mirroring `claude-cli` (hooks, resume, Telegram). No sandbox or `WaitingForInput` in v1.

## Implementation Progress

**Last synced with code**: 2026-07-05 (via `/validate-changes`)

- [x] M1 — `cli_session_manager.rs` module (+ `claude_cli_session` re-export shim)
- [x] M2–M9 — hooks, dispatch, argv, web UI, list-models, Telegram, config
- [x] Acceptance + Cypress tests (18 Rust + 3 Cypress)
- [x] `./test` green with `LIVEKIT_TESTKIT_WS_URL`
- [~] Deferred: `ConnectionScreen` legacy inline form third session type (CreateSessionPane is primary)

## Validation Results

### Change Validation (@validate-changes)

**Last run**: 2026-07-05  
**Status**: ✅ Passed (after fix)  
**Risk level**: 🟢 Low

| Package | Build | Clippy |
|---------|-------|--------|
| tddy-core | ✅ | ✅ |
| tddy-daemon | ✅ | ✅ |
| tddy-tools | ✅ | ✅ |
| tddy-web (Cypress) | ✅ | n/a |

**Issues found & resolved**:
- 🔴 **Fixed**: Web `CreateSessionPane` sent `new_branch_from_base` with empty `new_branch_name`; daemon rejected. Auto-default to `cursor-cli/{short_id}` in `cursor_cli_spawn.rs` + acceptance test.

**Accepted (matches claude-cli)**:
- Hook write failure logs warning and continues (same as `.claude/settings.local.json` path).
- `daemon_url` defaults to `http://127.0.0.1:{web_port}` when unset (operator configures `cursor_cli.daemon_url` for remote hosts).

**Out of PR scope**:
- `claude-via-livekit.sh` (untracked helper script)
- `packages/tddy-sandbox-recipes` export reorder (incidental)

### Test Validation (@validate-tests)

**Last run**: 2026-07-05  
**Status**: ✅ Passed

- 6 acceptance tests in `cursor_cli_session_acceptance.rs`
- 3 in `cursor_cli_hooks_acceptance.rs`
- 3 in `telegram_start_cursor_acceptance.rs`
- 3 Cursor stdin tests in `session_hook_cli.rs`
- 3 Cypress tests in `CreateSessionCursorCliAcceptance.cy.tsx`
- Fluent-tests structure; no `.skip`/`.only`; meaningful assertions

### Production Readiness (@validate-prod-ready)

**Last run**: 2026-07-05  
**Status**: ✅ Ready

- No mock/fake code in production paths
- No new TODO/FIXME in changed production files
- Sandbox explicitly rejected for `cursor-cli` (`failed_precondition`)

### Code Quality (@analyze-clean-code)

**Last run**: 2026-07-05  
**Overall score**: 8/10 ⭐

- `spawn_cursor_cli_session_inner` has many params (pre-existing pattern; `#[allow(clippy::too_many_arguments)]`)
- Telegram spawn mirrors claude-cli structure (acceptable duplication)
- Naming and module split clear after M1

## Refactoring Needed

_None blocking PR._

### Deferred (out of scope)

- [ ] `ConnectionScreen` legacy form: third session-type toggle (CreateSessionPane covers web flow)
