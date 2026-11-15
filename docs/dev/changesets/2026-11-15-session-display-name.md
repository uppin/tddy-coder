# 2026-11-15 — Sessions get a display name an operator, an orchestrator or the session's own agent can set

**Type:** Feature

PR [#910](https://github.com/uppin/tddy-coder/pull/910). Features:
[session-drawer.md § Session names](../../ft/web/session-drawer.md#session-names),
[session-notifications.md § One label, everywhere](../../ft/daemon/session-notifications.md#one-label-everywhere),
[project-concept.md § Project-home tools](../../ft/daemon/project-concept.md#project-home-tools).

A session carries an optional **display name**. When set it is the session's label everywhere —
the drawer row, the inspector, Telegram activity alerts, and what an orchestrator's tools report —
ahead of the derived one (repo basename → workflow goal → short id). The name is trimmed; an empty
name clears it; at most 80 characters, on one line. It is set from the web inspector, at a start,
by an orchestrator's `RenameSession`, or by the session's own agent with `tddy-tools rename-session`.

## What changed

- **`tddy-changeset`** — `SessionMetadata.display_name` (skipped when absent) and the
  `session_display_name` module: `DISPLAY_NAME_MAX_CHARS = 80`, `normalize_display_name`
  (`TooLong` / `ControlCharacter`, refused rather than cut) and `rename_session(session_dir, raw)`,
  which leaves `updated_at` alone. `session_label::session_display_label` takes `display_name` first.
- **`tddy-service`** — `SessionEntry.display_name = 39`, `StartSessionRequest.display_name = 49`,
  `SessionService.RenameSession` (registered in `unbundle_service_split`'s closed
  `RESIDUAL_METHODS`, the seventh method added deliberately), `ActivityService.RenameOwnSession`;
  TypeScript regenerated.
- **`tddy-agent-launch`** — `svc_rename_session.rs`; a start's name validated in
  `start_session_core_cancellable` before routing and recorded once after a local start
  (`record_the_started_sessions_name`); a named `tddy_coder` start refused
  (`refuse_a_named_tool_start`); the listing carries the name; `StartedSessionDetails` answers a
  `StartedSessionListing` with the name; the agent guidance names `tddy-tools rename-session`.
- **`tddy-session-lifecycle`** — `rename_session` through `SessionHandler`, `SessionServiceImpl`,
  `DaemonSessionHost` and `TestDaemon`; `RenameOwnSession` delegated locally by `PeerRoutedActivity`.
- **`tddy-session-activity`** — the listing's `SessionEntry.display_name`; `resolve_session_label`
  returns the name before any other lookup; `RenameOwnSession` (`service/own_session_name.rs`),
  gated like `ReportOperatorNotification`.
- **`tddy-tool-engine`** — `RenameSession` (`project_home_lifecycle_tools/rename_session.rs`), a
  lifecycle target tool beside `SetSessionMark`; `display_name` on `StartSessionArgs`,
  `ListedSession`, `SessionActivity`/`ActivityPage` and `StartLedgerRecord` (`#[serde(default)]`
  where a peer or an older record may lack it).
- **`tddy-daemon-rpc`** — the port's `rename_session`, the listing and follow reads carrying the name.
- **`tddy-tools`** / **`tddy-tools-cli`** — the `rename-session` subcommand over `notify`'s session
  and a `Transport` generalised to any `ActivityService` method; `RenameSession` in the closed-world
  admission list.
- **`tddy-session-web`** — `sessionDrawerLabel` puts `displayName` first; `SessionNameEditor` in
  the inspector's Details tab; Cypress spec and Storybook stories.
- **`tddy-session-create-web`** — the optional Name field (`CreateSessionNameField`).
- **`tddy-task`**, **`tddy-daemon-rpc`**, **`tddy-daemon`**, **`tddy-tools`** (tests) — flaky tests
  found while landing this made deterministic: `IdleTimeoutTracker::last_active_at(timeout,
  instant)` replaces millisecond idle windows (1 ms in `rpc_handlers_acceptance`, 500 ms, 80 ms,
  20 ms) with a tracker idle from the start under a one-minute timeout; the session-action job stub
  runs until the test releases it instead of sleeping 0.25 s.
- **`tddy-workflow-recipes`** — `schema-manifest.json` is written with keys in sorted insertion
  order, so it no longer changes with `serde_json`'s `preserve_order` feature (switched on by
  whichever crates share a cargo invocation).

## Backlog

- Resolved: none.
- Added: *A tool session cannot be named at its start*
  (`2026-11-15-a-tool-session-cannot-be-named-at-its-start`).
- Still open and now also covering names: *Session notifications — three follow-ups left open*
  (`2026-08-29-session-notifications-three-follow-ups-left-open`) — the Telegram session list and
  chain-parent buttons name sessions by their short ids, so they show no display name either.

## Code issues

- `tddy-agent-launch` `complexity-svc-start-session-core-start-session-core`:
  `start_session_on_request` 73 → 79 lines, `start_here_body` 274 → 274 (fn line to closing brace).
  Kept open.
- `tddy-tool-engine` `complexity-start-session-args-schema-schema-offering`: `schema_offering`
  191 → 195 lines. Kept open.
