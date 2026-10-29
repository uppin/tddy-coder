# PRD: #home-inbox sweep — what the stack left undone

**Created:** 2026-10-29
**Status:** 🚧 Planned
**Area:** daemon — project home, claude-cli sessions; desktop
**Stack:** `#home-inbox 11/11`, the sweep node

The developer chose these three TODOs (2026-10-29). Each closes a gap the stack recorded instead of
fixing.

## 1. A conversation spawner reads its children's activity

**Resolves:** [`docs/dev/todo/2026-10-29-conversation-spawner-cannot-read-its-childs-permission-request.md`](../../../dev/todo/2026-10-29-conversation-spawner-cannot-read-its-childs-permission-request.md)

A session that can spawn conversations, such as a `grill-me` session, may answer its child's
permission prompts and questions. It is offered `AnswerSessionPermission` and
`AnswerSessionQuestion`, but it cannot read the `request_id`. So its child waits the full
`answer_wait_secs` (default 120 s) on every prompt.

**Change:** the developer chose to give spawners a read tool. A spawner is offered
`ReadSessionActivity`, scoped to the sessions whose changeset names it as
`orchestrator_session_id`. Reading any other session is refused as permission denied, naming why.

**Acceptance:**
- A grill-me spawner reads its child's pending `request_id` and answers it.
- Reading a session it did not spawn is refused.

## 2. A session with no shell notifies its operator

**Resolves:** [`docs/dev/todo/2026-10-26-shell-less-sessions-cannot-notify-their-operator.md`](../../../dev/todo/2026-10-26-shell-less-sessions-cannot-notify-their-operator.md)

`tddy-tools notify` needs a shell. A session with a standing role, the project-home main, has none.
It is the agent with the most news for an absent operator.

**Change:** an MCP tool `NotifyOperator { title, body_markdown, level?, sound? }` is served to
sessions with a standing role. It publishes the same agent notification as `tddy-tools notify`,
through `ActivityService.ReportOperatorNotification` on the session's own daemon, with the
credential its session record holds. The guidance paragraph those sessions get names the tool
instead of the CLI. Sessions without the role are not offered it.

**Acceptance:**
- A project-home main's `NotifyOperator` publishes one `AGENT` attention notification. It carries
  the title and body, and reaches Telegram and the desktop as `tddy-tools notify` does.
- A session without the role is not offered the tool.

## 3. Clicking a desktop notification opens its session

**Resolves:** [`docs/dev/todo/2026-10-26-desktop-notification-click-needs-tauri-2-12.md`](../../../dev/todo/2026-10-26-desktop-notification-click-needs-tauri-2-12.md)

A Tddy Desktop notification does nothing when clicked. `open_session_route` exists and is tested,
but nothing calls it, because `tauri-plugin-notification` 2.4 reports no clicks.

**Change:**
- Tauri goes from 2.11.5 to 2.12. `Cargo.toml` already accepts `tauri = "2"`, and the toolchain is
  Rust 1.94, past the 1.90 it needs.
- `tauri-plugin-notification` goes to 2.5 or later.
- A `tap` on a notification the desktop showed opens its session: the payload carries the session
  id and host, and the handler calls `open_session_route`.

**Acceptance:**
- A tap's payload maps back to the right `SessionRoute`, and the handler calls
  `open_session_route` with it. This is tested without a GUI.
- The desktop crate builds and its suites pass on Tauri 2.12.
- **Manual:** on macOS, clicking a notification brings the window forward on that session.

## Also in this node (no user-visible change)

- Split the two files the stack grew past the 500-line budget. `exec_tool/project_home.rs` in
  `tddy-daemon-rpc` went from 641 to 889 lines. `project_home_tools.rs` in `tddy-tool-engine` went
  from 699 to 757.
- Fix the pending-question clear in `answer_permission`. After the hook wakes, it cleared the
  child's pending question unconditionally, and could delete a newer one. `8/11` fixed the same race
  in `answer_question`.
