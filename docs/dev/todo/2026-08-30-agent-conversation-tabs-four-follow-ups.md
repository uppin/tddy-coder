# 2026-08-30 — Agent conversation tabs — four follow-ups

**Category:** Future enhancement
**Source:** session-agent-conversation-tab changeset, 2026-08-30

- **A conversation still dies when the create-session pane opens.** `runtimeLayer` now holds one
  stable slot across every *base view* branch, so selecting a workflow session no longer unmounts the
  conversation bodies (and a body cancels its conversation as it unmounts). `isCreating` is the
  remaining hole: it skips the whole session-detail block, so opening "new session" cancels every open
  conversation while its tabs survive in state. Fixing it means hoisting the runtime layer above the
  `PanelGroup` in `SessionMainPane`, which breaks the Code-pane split — a layout change, not a wiring
  one, and bigger than the gap.
- **The header's Add-agent button renders where there is no tab strip.** Any selected session with a
  client gets it, including workflow, PR-Stack and dormant sessions. The attach succeeds, the picker
  closes, and nothing visible happens. Hide the button where no tab strip exists, or say in the UI why
  the agent cannot be talked to there.
- **The conversation body's inner test ids are not keyed by conversation.**
  `agent-conversation-input` / `-transcript` / `-error` / `-turn-<i>` are unscoped while every open
  conversation stays mounted, so two open tabs make them ambiguous and a spec that prompts with two
  tabs open would fail on a multi-element match. No current spec does. Key them by `conversationId`,
  or scope the page object's helpers with `.within(pane(id))`.
- **The per-session conversation maps are never pruned.** `agentConversations` / `activeConversations`
  in `SessionMainPane` keep entries for sessions that have left the list. Growth is trivial, but a
  *resumed* session keeps its `sessionId`, so its tabs come back pointing at conversations the daemon
  dropped long ago, which then re-open under ids it has already seen.
