# 2026-11-07 — Supervisor calls and session starts have deadlines

**Type:** Bug Fix

`#agent-tools` 23/28 ([#831](https://github.com/uppin/tddy-coder/pull/831)), branch
`feature/agent-tools/bounded-waits`, based on `feature/agent-tools/host-registry`.

Packages: `tddy-supervisor`, `tddy-daemon-kernel`, `tddy-agent-launch`, `tddy-telegram-control`;
wiring tests in `tddy-session-lifecycle`.

## What changed

- **Supervisor client.** `SUPERVISOR_CALL_DEADLINE` (60 s) on every unary call;
  `SUPERVISOR_CLONE_PROGRESS_DEADLINE` (300 s) on a clone's opening + first event and on each gap
  after. A miss is `Unavailable` naming the method and deadline. Test setters `with_call_deadline`,
  `with_clone_progress_deadline`.
- **Start deadline.** `DaemonConfig::session_start_timeout_secs` (600, `0` → 1 s).
  `tddy_agent_launch::start_deadline::StartDeadline` runs `start_here`: cancel at expiry, 30 s grace
  (`START_CANCEL_GRACE`), undo a session that came back, drop one that did not, `DEADLINE_EXCEEDED`
  naming the running step.
- **Token-free undo.** `undo_a_timed_out_start` → `remove_what_a_start_created` (by OS user:
  `branch_a_start_cut_as`, `delete_session_as`). `undo_a_cancelled_start` now runs the same teardown
  after `os_user_of_session_token`; `branch_a_start_cut` (token-keyed) is gone.
- **Telegram.** `START_DEADLINE` removed; the daemon's refusal is relayed.

## Resolved backlog

- `docs/dev/todo/2026-11-04-acp-launch-waits-have-no-deadline.md` — the supervisor half bounded here;
  the ACP `open_session` half was already bounded (`SESSION_OPEN_DEADLINE`).
- `docs/dev/todo/2026-11-06-generic-session-start-has-no-deadline-of-its-own.md`.

## Decisions

- Deadlines on the supervisor client are constants, not config (the client has no config handle).
- The `clone_askpass` host-key hang did not reproduce (macOS, nix OpenSSH 10.2p1, 10/10 in 2.4–3.1 s);
  `clone_planner.rs` untouched. `tddy-askpass` refuses any non-passphrase prompt.
- The deadline polls the start in its own task and reads the progress channel there, so the named
  step is exact; `sleep_until` is raced rather than `timeout`, so the grace keeps polling.

## Left open

- `docs/dev/todo/2026-11-07-operator-start-undo-of-peer-held-parts.md` — `TODO(operator-start-undo)`,
  a planned follow-up stack node.
- `docs/dev/todo/2026-11-07-rpc-dropped-call-leaves-pending-entry.md` — `tddy-rpc` engine leak on a
  dropped call.
- The wiring tests' stuck step is an SSH clone to 192.0.2.1 (TEST-NET-1); where that is refused at
  once the tests fail loudly. The grace-then-drop path is unit-tested only.
- `TODO(start-cancel)`: a start dropped in the grace can leave a partially cut worktree.

## Tests

`tddy-supervisor/tests/supervisor_call_deadline.rs` (5), `tddy-agent-launch/tests/session_start_deadline.rs`
(7), `tddy-daemon-kernel` config tests (3), `tddy-session-lifecycle/tests/session_start_deadline_acceptance.rs` (3).
