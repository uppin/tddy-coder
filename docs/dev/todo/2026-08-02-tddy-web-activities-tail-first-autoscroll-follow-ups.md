# 2026-08-02 — tddy-web — activities tail-first / autoscroll follow-ups

**Category:** Future enhancement
**Source:** activities-tail-first-autoscroll changeset, 2026-08-02

- **The live, interactive chat surfaces still never scroll.** `AgentChatView` gains sticky-bottom
  follow only in `readOnly` mode; `AgentChat`, `WorkflowChatScreen` and the PR-Stack chat render the
  same unmanaged `overflow-y-auto` list and stay pinned to the top. The same helpers
  (`src/lib/scrollFollow.ts`) apply, but the live surfaces add composer focus, the elicitation
  composer and the send round-trip to reason about, so they were deliberately left out of scope.
- **The loaded range is not virtualized.** Paging bounds what is *fetched*; once an operator has
  paged back several thousand entries, every one of them is still mounted. Windowing the rendered
  range is the next cost to pay, and it interacts with the prepend scroll anchor.
- **The read position is not persisted across a session switch.** Switching away and back re-opens
  at the tail even though the registry still holds the loaded range and its cursor. Persisting the
  offset per session in `agentActivityRegistry` would make switch-back resume where the operator
  left off.
- **`StreamAcpReplay` still cannot peer-forward.** `TAIL_THEN_LIVE` inherits the existing
  `TODO(acp-replay)` limitation (the forwarding primitive's idle deadline is sized for a
  short-lived stream). `GetAcpReplayPage` is unary and forwards, so a cross-host session can page
  its history but cannot open the tail feed — the asymmetry is worth closing.
- **Importing `src/index.css` into the component harness is still deferred.** This changeset works
  around it by declaring the transcript's flex/overflow inline (see the changeset's Decisions). That
  duplicated declaration can be deleted once the harness loads the stylesheet — see the entry below.
