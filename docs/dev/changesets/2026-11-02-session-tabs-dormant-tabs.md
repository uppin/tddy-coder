# 2026-11-02 — A stopped session keeps its tabs

**Type:** Feature

`#session-tabs` 1/7 ([#804](https://github.com/uppin/tddy-coder/pull/804)). A dormant session
(`is_active == false`) keeps a working tab strip: Agent (the recorded transcript), its daemon
terminals, agent conversations and "+". Across:

- `tddy-session-lifecycle` — `CliManagerTerminalRoster::start` takes the live main terminal's
  worktree, or for a stopped session `recorded_worktree(tddy_data_dir, session_id)`
  (`connection_service/svc_terminal_ports/recorded_worktree.rs`): the id must be one path segment
  (`INVALID_ARGUMENT`), the worktree is `tddy_core::repo_root_for_session` of
  `<data dir>/sessions/<id>`, and nothing recorded is `FAILED_PRECONDITION` naming the session. The
  roster gains the daemon's `tddy_data_dir`. Tests: 4 unit (`recorded_worktree`), 5 acceptance
  (`dormant_session_tabs_acceptance`: bash and harness tabs start in the recorded worktree, no
  worktree and unknown session refused naming it, a live session unchanged).
- `tddy-web` — `DormantSessionRuntime` is the stopped strip: `SessionTerminalTabs` +
  `useSessionTerminals` + `GrpcSessionTerminal` over the owning daemon's terminal client, its Agent
  tab a render slot (`agentTab`) that `SessionMainPane` fills with `SessionActivitiesPane`. It
  carries full screen, the shortcut drawer, Files-tab Insert (via `onRegisterInsert`) and the claim
  toast for its bash tabs; the shared chrome (`PaneFullscreenExit`, `TerminalControlLayer`) moved to
  `SessionPaneChrome.tsx`. Agent-conversation bodies moved out of `SessionRuntime` into
  `SessionAgentConversationLayer`, rendered once by `SessionMainPane` and portalled into a
  per-session host element that the strip showing the session adopts — so neither Resume nor leaving
  the session cancels a conversation. `SessionRuntime` drops its unused `callerLogin` prop and gains
  `conversationHost` / `hostsConversations`. Test fake: `leaseHeldBy` on the in-memory
  `TerminalSessionService`. Tests: 11 Cypress component (`DormantSessionTabsAcceptance.cy.tsx`).

Decisions: the session's record is the source when `main` is gone, not a path cached in the manager,
which a daemon restart empties; no substitute directory — Resume falls back to the session dir, tabs
refuse; the session dir is the daemon's, so the roster needs no OS user (`TerminalLaunch.os_user`
moved to `#session-tabs` 3/7, its only consumer); conversation bodies above both strips via stable
portal hosts, since a body cancels on unmount and a portal whose container changes remounts.

Backlog: `2026-08-30-agent-conversation-tabs-four-follow-ups` narrowed — its "Add-agent button
renders where there is no tab strip" bullet now covers workflow and PR-Stack sessions only.

Code issues: `SessionMainPane.tsx` grew 468 → 503 lines and is recorded as
`packages/tddy-web/docs/code-issues/oversized-file-session-main-pane.md`; not split here because
`#session-tabs` 6/7 edits the same file. `oversized-file-connection-service` unchanged (the new
module is a submodule of `svc_terminal_ports`, no line added to `connection_service.rs`);
`oversized-file-sessions-drawer-screen` unchanged (file not touched).
