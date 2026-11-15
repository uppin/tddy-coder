# Sessions get a display name an operator, an orchestrator or the session's own agent can set

**Type:** Feature
**Packages:** `tddy-service`, `tddy-changeset`, `tddy-session-activity`, `tddy-agent-launch`,
`tddy-session-lifecycle`, `tddy-tool-engine`, `tddy-daemon-rpc`, `tddy-tools`, `tddy-tools-cli`,
`tddy-session-web`, `tddy-session-create-web`, `tddy-storybook`
**Feature docs:** [session-drawer.md § Session names](../../ft/web/session-drawer.md#session-names),
[session-notifications.md § One label, everywhere](../../ft/daemon/session-notifications.md#one-label-everywhere),
[project-concept.md § Project-home tools](../../ft/daemon/project-concept.md#project-home-tools)

## Problem (State A)

No session has a name. Every surface derives a label: the basename of `repo_path`, else
`workflow_goal` (unless the `—` placeholder), else the first eight characters of `session_id`. The
rule lives twice — `sessionDrawerLabel` (web) and `session_display_label` (Rust, used for Telegram
activity alerts) — and an operator with three sessions on worktrees named after branches has no way
to call one "the login fix". An orchestrator cannot name the sessions it starts either, and an agent
cannot say what its own session is about.

## Change (State B)

A session carries an optional **display name**. When set, it is the session's label everywhere,
ahead of the derived one.

### Store — `tddy-changeset`

- `SessionMetadata.display_name: Option<String>` in `.session.yaml`, `skip_serializing_if = None`,
  so an unnamed record reads and writes exactly as before. (An older daemon cannot read a *named*
  record: the struct is `deny_unknown_fields`, as with `orchestration_mark`.)
- New module `session_display_name`: `DISPLAY_NAME_MAX_CHARS = 80` (characters, not bytes),
  `normalize_display_name` (trim; empty → `None`; over the limit → `TooLong`; any control character →
  `ControlCharacter` — refused, never cut or cleaned) and `rename_session(session_dir, raw)`, which
  every writer goes through. Renaming does not bump `updated_at`: naming a session is not activity in
  it, and `updated_at` orders branch ownership.
- `session_label::session_display_label` takes `display_name` first. Its web twin
  `sessionDrawerLabel` takes `SessionEntry.displayName` first; the parity cases are mirrored in
  `tests/session_display_label_acceptance.rs` and `sessionDrawerLabel.test.ts`.

### Wire — `tddy-service`

- `SessionEntry.display_name = 39`, filled by `ListSessions` from the record (and so by every
  cross-host listing, which relays the entries).
- `StartSessionRequest.display_name = 49`, recorded at creation.
- `SessionService.RenameSession(RenameSessionRequest{session_token, session_id, display_name}) →
  {display_name}` — answered by the daemon holding the session; the web routes it to the session's
  owning host as it does `DeleteSession`. Registered in `unbundle_service_split`'s closed
  `RESIDUAL_METHODS` list (the fourth method added deliberately).
- `ActivityService.RenameOwnSession(RenameOwnSessionRequest{session_id, hook_token, os_user,
  display_name}) → {display_name}` — the agent's own path, authenticated exactly like
  `ReportOperatorNotification` (agent declares `operator_notify`; constant-time `hook_token` match),
  so an agent can name its own session and no other.

### Daemon — `tddy-agent-launch`, `tddy-session-lifecycle`, `tddy-session-activity`

- `svc_rename_session.rs`: the `RenameSession` handler (token → OS user → session dir →
  `rename_session`). NOT_FOUND for a session this daemon does not hold, INVALID_ARGUMENT for a
  refused name.
- Start: the requested name is validated in `start_session_core_cancellable` before anything is
  created, and recorded once, centrally, after a **local** start succeeds (a forwarded start is named
  by the peer that creates it). A `tddy_coder` tool-session start that carries a name is refused
  (`refuse_a_named_tool_start`): its `.session.yaml` is written by its own process after spawn, so a
  name written by the daemon would race it and be lost.
- `resolve_session_label` (Telegram activity alerts) returns the name before reading anything else.
- `own_session_name.rs`: the `RenameOwnSession` handler.

### Tools — `tddy-tool-engine`, `tddy-daemon-rpc`, `tddy-tools`

- `RenameSession` exec tool (`project_home_lifecycle_tools/rename_session.rs`): `session_id`,
  `display_name`, `daemon_instance_id?` → `{session_id, display_name}` (`null` when cleared). A
  lifecycle target tool beside `SetSessionMark`: same project-home/orchestrator admission, same
  forward to the named host; the name is normalised before the port is asked. A running session is
  renamed like a stopped one. Registered in the tddy-tools closed-world admission list and the
  read-only allowed-tools list.
- `StartSession` takes `display_name` (offered in its schema; `start_session_args_parity` passes).
- `ListProjectSessions` rows, `ReadSessionActivity` pages and `GetSessionStart` records carry
  `display_name` (`null` when unnamed). `ListedSession.display_name` and
  `StartLedgerRecord.display_name` are `#[serde(default)]`, so an older peer's rows and an older
  ledger record still read. `StartedSessionDetails::branch_and_worktree` became `listing`, returning
  a `StartedSessionListing { branch, worktree, display_name }`.
- `tddy-tools rename-session <NAME>`: the agent's own rename, over the same environment, session
  pointer and transport as `tddy-tools notify` (agent-tool socket, else daemon URL). Prints
  `Session renamed: <name>` or `Session name cleared`; exits non-zero with the daemon's reason.
  The agent guidance appended to a claude-cli agent's system prompt names it.

### Web — `tddy-session-web`, `tddy-session-create-web`, `tddy-storybook`

- `sessionDrawerLabel` puts `displayName` first.
- `SessionNameEditor` at the top of the inspector's Details tab: double-click the label or
  **Rename**; Enter saves, Escape cancels, an empty value clears back to the derived label; a refusal
  is shown inline with the editor kept open. Saves through `client.renameSession` on the owning host,
  then refreshes the session list. Cypress component spec + Storybook stories.
- The create pane has an optional **Name** field (hidden for a `tddy_coder` session type), sent as
  `StartSessionRequest.display_name`.

## Acceptance tests

| Suite | What it pins |
|---|---|
| `tddy-changeset/tests/session_display_name.rs` | normalisation rules; rename changes only the name; clear writes no key; refused name keeps the old one |
| `tddy-changeset/tests/session_display_label_acceptance.rs` | name first; trimmed; blank ignored (mirrors the web) |
| `tddy-daemon-rpc/tests/session_display_name_acceptance.rs` | RPC rename/clear/limit/not-found/unauthenticated; start with a name; invalid name creates nothing; tool rename on this host and another; listing, follow and start ledger report the name |
| `tddy-tool-engine/tests/project_home_lifecycle_tools.rs` | `RenameSession` rules, refusals before the port, target-tool routing, schema |
| `tddy-tool-engine/tests/session_display_name_reports.rs` | `StartSession` argument; older peer rows; ledger record names the session |
| `tddy-session-activity/tests/own_session_rename_acceptance.rs` | agent rename: token, capability, limits |
| `tddy-tools-cli/tests/rename_session_cli_acceptance.rs` | the CLI as the agent runs it |
| `tddy-session-lifecycle` `list_sessions_unit_tests.rs`, `session_notification_label_unit.rs` | listing carries the name; Telegram label prefers it |
| `tddy-session-web` `sessionDrawerLabel.test.ts`, `SessionNameEditor` Cypress spec | label rule; inline rename interaction |
| `tddy-session-create-web` request builder test | `displayName` on the start request |

## Left open

- TODO(session-display-name): the other Telegram surfaces — the session list, chain-parent buttons
  — still use the short-id label (`docs/ft/daemon/session-notifications.md` already records them as
  a follow-up); they should take `session_display_label` with the name.
- TODO(session-display-name): naming a `tddy_coder` tool session at start is refused; it needs the
  name passed to the spawned `tddy-coder` (a `--display-name` flag written by
  `write_initial_tool_session_metadata`). Renaming it once it runs works.
- Renaming is not broadcast: other open browsers see the new name on their next list refresh, as
  they do a mark.
