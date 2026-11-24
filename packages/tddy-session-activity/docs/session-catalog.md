# The session catalog's daemon side (tddy-session-activity)

Four modules read, enrich and delete the sessions under an OS user's sessions tree. They serve
`ListSessions` and `DeleteSession`, which `tddy-session-lifecycle`'s `DaemonSessionHost` answers, and
the delete paths of the daemon's other services. None of them holds session-host state.

| Module | Holds |
|---|---|
| `user_sessions_path` | resolving an OS user to their sessions directory: a re-export of `tddy_daemon_kernel::user_paths` (`home_dir_for_user` and the rest), `username_for_uid` (`getpwuid_r`), and `tddy_data_root_matching_child` |
| `session_reader` | `SessionEntry` and `list_sessions_in_dir`, which read each `<session_id>/.session.yaml`; `DaemonSessionListing::new(live)`, this daemon's `tddy_worktree_service::branch_owner::SessionListing`; and `is_pid_alive`, the probe a `tool` session's deletion uses. `list_sessions_in_dir(sessions_base, &LiveSessions)` takes the daemon's live set as a required argument and marks a session `is_active` when the set holds its id: the recorded pid is never consulted ([Session liveness](../../../docs/ft/daemon/session-liveness.md)) |
| `session_list_enrichment` | the display fields a `ListSessions` row carries beyond `.session.yaml` — goal, workflow state, elapsed time, agent, model, the stack plan and a pending elicitation — read from the session's `changeset.yaml` (parity with the TUI's status bar), and `apply_session_list_status_to_proto`. Both take the host's `CodingAgentRegistry`: a session whose type a coding agent declares (`for_session_type`) runs no workflow, so it has no goal or workflow state |
| `session_deletion` | filesystem-safe deletion: validating the session id, resolving the directory under the sessions tree, stopping a `tool` session's recorded live PID (SIGTERM, a wait, then SIGKILL; `signal_pid`) — the daemon stops every other session's processes before the directory goes, closing the session room, tearing down a workspace sandbox, and deciding which session types remove a daemon-managed worktree |

`session_reader` carries the name the session was given (`SessionEntry.display_name`, from
`.session.yaml`'s `display_name`; empty for an unnamed session) onto the `ListSessions` row as
`display_name` — the label every surface puts first.

`session_reader` also answers, per session, whether its agent **reads this host's filesystem**
(`SessionEntry.reads_host_paths`, carried onto the `ListSessions` row as `reads_host_paths`):
`tddy_changeset::reads_host_paths` over the session's `.session.yaml`, false for a jailed agent
(`sandbox`), a managed-codebase one, a split agent (`codebase_daemon_instance_id`, which covers the
sandboxed-codebase placement too), one whose tools run over SSH (`ssh_config_host`) and a `workspace`
checkout. Tddy Desktop types a dropped file's real path only for a session that does; every other
session has the file uploaded into its session directory.

`session_list_enrichment` also answers `SessionEntry.holds_worktree`: true when the session's type owns
a worktree (`session_deletion::worktree_removal_applies_to`), it records one, its directory holds no
`worktree-removed` marker, and — for a local session — the path is a daemon-managed worktree (under
`.worktrees`) that exists. The drawer offers a closed session's trash bin by it.

`session_list_enrichment` reads a session's pending-elicitation flag through this crate's
`pending_elicitation` module, which reads the `pending_elicitation` field of `.session.yaml` (`false` when the metadata cannot be read). It lives here rather than with Telegram
because every daemon's session list shows it.

`tddy-session-lifecycle` re-exports all four by name
(`pub use tddy_session_activity::{session_deletion, session_list_enrichment, session_reader,
user_sessions_path};`), so `tddy_session_lifecycle::session_reader::…` and the other paths resolve
for `tddy-daemon`, `tddy-daemon-rpc`, `tddy-telegram-control` and `tddy-worktree-service`'s tests.
`signal_pid` is `pub` because lifecycle's `CliSessionManager` stops terminals with it.

## Why here and not in `tddy-session-catalog`

`tddy-session-catalog` is the catalog `tddy-coder` and `tddy-bsp` read. `session_reader` and
`user_sessions_path` need `tddy-daemon-kernel`, `tddy-worktree-service`, `tddy-core`, `anyhow` and
`libc`; in the catalog they would add 85 packages to `tddy-bsp`'s own graph. Every caller is on the
daemon side, and `session_deletion` needs `is_pid_alive` for a `tool` session, so the four live together here.

They are what this crate's dependencies on `tddy-projects` (`project_storage`), `tddy-session-files`
(`session_context_docs`), `tddy-daemon-sandbox` (`RUNNER_PID_FILE`), `tddy-daemon-livekit`
(`SessionRoomRegistry`), `chrono` and, on unix, `libc` are for. Nothing that depends on this crate is
below it: its dependents are lifecycle, `tddy-daemon-rpc`, `tddy-daemon`, `tddy-desktop` and
`tddy-telegram-control`.

## Tests

| Where | Tests |
|---|---|
| `src/session_deletion.rs` (unit) | 12 |
| `src/session_list_enrichment.rs` (unit) | 27 |
| `src/session_reader.rs` (unit) | 3: a listing marks a session active from the live set alone — a recorded pid that is a running process does not, a live session with no recorded pid does |
| `tests/worktree_removal_eligibility.rs` | 8: which session types remove a daemon-managed worktree on delete — `claude-cli`, `opencode-cli`, `muse-cli`, `warp-cli` and `workspace` do (`worktree_removal_applies_to`); `tool`, an unrecorded type and `cursor-cli` do not |

These 50 are the crate's only tests; the activity service itself has none of its own (see the
[README](../README.md#testing)).

## Related

- [activity-service.md](./activity-service.md) — the service the rest of this crate serves
- [`tddy-session-lifecycle` session service](../../tddy-session-lifecycle/docs/session-service.md) —
  `ListSessions` and `DeleteSession`
- [`tddy-session-catalog`](../../tddy-session-catalog/) — the catalog `tddy-coder` reads
