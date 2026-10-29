# Changeset: #home-inbox sweep

**Created:** 2026-10-29
**Status:** 🚧 Planned (pass 1: docs only)
**Type:** Sweep (feature + refactor + fix)
**Complexity**: 5. It covers three independent features, two file splits, and a dependency upgrade (Tauri 2.12 plus the notification plugin) across about eight packages.
**Stack:** `#home-inbox 11/11` (sweep), branch `feature/home-inbox/sweep`, base `feature/home-inbox/cross-host-wait`
**PRD:** [PRD-2026-10-29-home-inbox-sweep.md](../../ft/daemon/1-WIP/PRD-2026-10-29-home-inbox-sweep.md)

## Initial Discovery

[2026-10-29-home-inbox-sweep-initial-discovery.md](./2026-10-29-home-inbox-sweep-initial-discovery.md)

## Prerequisites (what this node resolves)

| Record | Verdict |
|---|---|
| [`docs/dev/todo/2026-10-29-conversation-spawner-cannot-read-its-childs-permission-request.md`](../todo/2026-10-29-conversation-spawner-cannot-read-its-childs-permission-request.md) | ✅ RESOLVED HERE. The developer chose a scoped read tool (2026-10-29). |
| [`docs/dev/todo/2026-10-26-shell-less-sessions-cannot-notify-their-operator.md`](../todo/2026-10-26-shell-less-sessions-cannot-notify-their-operator.md) | ✅ RESOLVED HERE |
| [`docs/dev/todo/2026-10-26-desktop-notification-click-needs-tauri-2-12.md`](../todo/2026-10-26-desktop-notification-click-needs-tauri-2-12.md) | ✅ RESOLVED HERE. The developer approved the Tauri upgrade (2026-10-29). |
| [`packages/tddy-daemon-rpc/docs/code-issues/oversized-file-exec-tool-project-home.md`](../../../packages/tddy-daemon-rpc/docs/code-issues/oversized-file-exec-tool-project-home.md) | ✅ RESOLVED HERE. Split along the record's own seams (B listing, C caller/log, plus the stack's terminal, inbox and answer port impls). |
| [`packages/tddy-tool-engine/docs/code-issues/oversized-file-project-home-tools.md`](../../../packages/tddy-tool-engine/docs/code-issues/oversized-file-project-home-tools.md) | ✅ RESOLVED HERE. Split along seams A (target host), B (catalogue) and C (listing types). |
| `answer_permission`'s unconditional pending-question clear, found at `8/11`'s validate | ✅ RESOLVED HERE |
| [`packages/tddy-tool-engine/docs/code-issues/oversized-file-lib.md`](../../../packages/tddy-tool-engine/docs/code-issues/oversized-file-lib.md), [`packages/tddy-task/docs/code-issues/oversized-file-terminal-capture.md`](../../../packages/tddy-task/docs/code-issues/oversized-file-terminal-capture.md) | Not in scope. The stack grew them only by single lines (`lib.rs` 885 → 888), or not at all (`terminal_capture.rs` ends at 712, below its base of 728). |

## Affected Packages

- [ ] `tddy-tool-engine`: spawner offer of `ReadSessionActivity`, the `NotifyOperator` tool def and dispatch, and the `project_home_tools.rs` split
- [ ] `tddy-daemon-rpc`: the scoped read check for spawners, `NotifyOperator` execution, and the `exec_tool/project_home.rs` split
- [ ] `tddy-session-activity`: the conditional clear in `answer_permission`
- [ ] `tddy-agent-launch`: guidance for standing-role sessions names `NotifyOperator`
- [ ] `tddy-tools`: the MCP serving and admission pins for `NotifyOperator`
- [ ] `tddy-desktop`: Tauri 2.12, notification plugin 2.5+, and the tap handler
- [ ] `Cargo.lock`

## Boundaries

- Nothing outside the three TODOs, the two splits and the race fix.
- **The splits are pure moves.** No behaviour changes, and they are done with the restructure engine where it accepts the move (`code-restructuring` skill). A move the engine refuses needs the developer's consent; see memory `restructure-refusal-needs-consent`.

## Dependencies

| Parent node | Consumed |
|---|---|
| `2/11`, `5/11`, `6/11`, `7/11`, `8/11`, `9/11` | the files being split; `ReadSessionActivity` gating; `answer_permission` |
| `1/11` agent-notify | `ReportOperatorNotification` and its credential check; the guidance paragraph |
| `4/11` desktop-notify | `native_notifications.rs`, `open_session_route`, `TauriNotifier` |
| `10/11` cross-host-wait | sits below; the splits come after it so they include its edits |

## Draft PR contract

Pass 2 publishes:
- the `NotifyOperator` def and the port surface;
- the spawner read-scope surface;
- the tap-to-route mapping signature.

Each comes with failing tests. The splits and the race fix are not contract surface; they land in
green with their tests.

## Testing Strategy

- **Acceptance:**
  - a spawner reads its child's `request_id` and answers it, and is refused for a session it did not spawn;
  - a project-home main's `NotifyOperator` publishes one agent notification, and is not offered to a session without the role;
  - a tap payload maps to its `SessionRoute`, and the handler calls `open_session_route`;
  - the permission clear keeps a newer pending question.
- **Splits:** the existing suites stay green, unchanged.

## TODO

- [ ] Pass 2: contract and failing tests
- [ ] TDD Green, including the splits and the race fix
- [ ] `/validate-changes`, `/pr-wrap`
- [ ] Manual: macOS notification click

## Validation Results

Not yet run.
