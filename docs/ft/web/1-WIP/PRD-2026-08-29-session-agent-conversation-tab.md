# PRD: Talk to a session's attached agent from a conversation tab

**Created:** 2026-08-29
**Product Area:** web
**Status:** WIP

## Summary

The session header's **Add agent** button stops spawning a peer *session* on the worktree and starts
**attaching a roster agent** to the session the operator is already looking at — then opens a
**conversation tab** in that session's tab strip, where the operator can prompt the attached agent
and read its answer stream in.

## Background

`SessionMainPane`'s header carries an **Add agent** button that today navigates to
`#/sessions/:id/add-agent`, which renders `CreateSessionPane` in `peerMode` and calls **`StartSession`
with `orchestrator_session_id`** — spawning a whole second coding session on the same worktree. That
predates the **session agent roster**
([docs/ft/daemon/session-agent-roster.md](../daemon/session-agent-roster.md)), which is the mechanism
the product now has for "give this session another agent": `AttachSessionAgent` adds a specialized
agent to the *existing* session, with a clone provisioned on its owning host when it is remote.

Two flows now answer the same operator question — "I want another agent on this work" — and they
answer it differently. The peer-spawn flow is the older and the weaker of the two: it costs a whole
session, it duplicates the worktree relationship the roster models properly, and its result is not
addressable by the main agent as a tool. The roster flow is the one the daemon, `tddy-tools` and the
inspector already speak.

The roster's own surface (`SessionAgentRosterPane`, in the Inspector's Agents tab) can already attach
and detach. What it cannot do is let the operator **talk to** the agent it just attached. The daemon
does expose the conversation RPCs — `OpenAgentConversation`, `PromptAgentConversation`,
`CancelAgentConversation` — but until now only the in-jail `tddy-tools` has called them, on the main
agent's behalf. Nothing in `packages/tddy-web/src` calls them at all.

### What is deliberately *not* being built, and why

The obvious reading of "show the attached agent's transcript" is a replay of what the **main agent**
asked the sub-agent. That is not buildable from the web today, and the reason is worth recording so
it is not re-attempted:

- A roster agent has **no session directory and no transcript**. `StreamAcpReplay` resolves its data
  purely from `unified_session_dir_path(sessions_base, session_id)`
  (`packages/tddy-daemon/src/connection_service.rs:13518`), and the only non-test caller of
  `append_acp_frame` in the repo is the coder process
  (`packages/tddy-coder/src/session_participant/acp_transcript.rs:42`). The daemon never appends one
  for an agent conversation.
- `SessionAgentEntry` (`packages/tddy-service/proto/connection.proto:355-393`) carries **no
  conversation id**, so there is nothing to key a per-agent replay by. `conversation_id` lives only
  in the daemon's in-memory `agent_conversations` map (`connection_service.rs:1064`), is never
  persisted, and has no lookup RPC.
- An agent's answer text exists only in the one-shot `mpsc` behind the `PromptAgentConversation` call
  that asked for it (`connection_service.rs:10256`) and is discarded after framing. The only thing
  that reaches a third party is a ≤120-char `last_activity.summary` on the roster.

So this PRD builds the conversation the web **can** hold: the operator's own. The main agent's use of
its sub-agents stays visible where it already is — as the roster row's status badge and last-activity
line. Making the main agent's sub-agent turns replayable is a daemon change and is out of scope here.

## Requirements

### Functional Requirements

- [ ] The session header's **Add agent** button opens an agent picker fanned out across every
      common-room daemon, the same catalog the roster pane's picker offers.
- [ ] The picker states what the main agent loses — the tools the picked agent `replaces` — before
      the operator confirms, exactly as the roster pane's picker does.
- [ ] Confirming calls **`AttachSessionAgent`** on the *current* session with the picked agent's
      **qualified** `agent_id`, routed to the session's facilitating daemon.
- [ ] A successful attach calls **`OpenAgentConversation`** with a caller-minted `conversation_id`
      and opens a **conversation tab** in that session's tab strip, focused.
- [ ] The conversation tab renders the operator's prompts and the agent's answers as a transcript,
      with a composer to send the next prompt.
- [ ] Sending a prompt calls **`PromptAgentConversation`**; the streamed `content_chunk` frames
      accumulate into one agent turn, and the final frame's `stop_reason` is recorded on it.
- [ ] An answer of zero length still renders as one completed agent turn — the daemon guarantees
      exactly one frame, so "said nothing" is never shown as "nothing arrived".
- [ ] Closing a conversation tab calls **`CancelAgentConversation`**, removes the tab, and returns
      focus to the Agent terminal tab.
- [ ] Attaching an agent that already has a tab open **focuses that tab** rather than opening a
      second one — attaching twice is a no-op on the roster, and the UI must not imply otherwise.
- [ ] The legacy peer-spawn flow is gone: the header button no longer opens `CreateSessionPane`, and
      `StartSession` is never called with an `orchestrator_session_id` from this button.

### Non-Functional Requirements

- [ ] Pure web. No proto change, no Rust change — every RPC used already exists and is already
      reachable with a `session_token`.
- [ ] A failed open, a failed attach and a failed prompt are each surfaced as a message naming the
      failure. None of them is shown as an empty transcript.
- [ ] Switching sessions or tabs does not tear a conversation down; only closing its tab does.

## Acceptance Criteria

1. Clicking **Add agent** in the session header opens the agent picker and does **not** open the
   session-creation pane.
2. The picker names the tools the main agent loses to the picked agent before the operator confirms.
3. Confirming sends `AttachSessionAgent` for the selected session under the picker's qualified
   `agent_id`.
4. A successful attach opens a conversation tab for that agent, and the tab is focused.
5. The conversation tab's body opens the conversation under the id the tab is keyed by.
6. A prompt typed into the composer is sent as `PromptAgentConversation` and appears in the
   transcript as an operator turn.
7. The agent's answer accumulates across `content_chunk` frames into a single agent turn, marked
   complete with its `stop_reason` when the final frame arrives.
8. An empty answer renders as one completed agent turn, not as an absent one.
9. A prompt that fails renders the failure, and the transcript keeps the operator turn that provoked
   it.
10. Closing the tab cancels the conversation and returns focus to the Agent tab.
11. Attaching an already-attached agent focuses its existing tab instead of opening a second.
12. `#/sessions/:id/add-agent` is no longer a route, and `CreateSessionPane` no longer has a peer
    mode.

## Related

- [Session agent roster (daemon)](../daemon/session-agent-roster.md) — the roster this attaches to.
- [Session terminal tabs](../web/session-terminal-tabs.md) — the tab strip this adds a tab kind to.
- [Agent activity pane](../web/agent-activity-pane.md) — the ACP transcript surface this is *not*.
