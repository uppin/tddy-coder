# Agent conversation tabs (attaching a roster agent from the session header)

How the web attaches a **session agent** and holds a conversation with it. Product behaviour lives in
[session-drawer.md § Add agent](../../../docs/ft/web/session-drawer.md); the roster itself is
[session-agent-roster.md](../../../docs/ft/daemon/session-agent-roster.md). This file is the part that
is easy to break from the outside: the invariants three components share.

## What this is not

It is **not** a replay of the main agent's use of its sub-agents. There is nothing to replay:

- a roster agent has no session directory, and nothing in the daemon appends an ACP frame for one —
  the only non-test caller of `append_acp_frame` in the repo is the coder process;
- `SessionAgentEntry` carries no conversation id, so there is no key a per-agent replay could use;
- an answer exists only in the one-shot channel behind the `PromptAgentConversation` call that asked
  for it, and is discarded once framed.

What reaches a third party is the roster row's status badge and its ≤120-char last-activity line. The
conversation in a tab is the **operator's own**. Do not reach for `useAcpReplay` here; it has nothing
to read.

## The three pieces

| Piece | File | Owns |
|---|---|---|
| Tab list + attach | `src/components/sessions/SessionMainPane.tsx` | which conversations exist, per session, and which is focused |
| Tab strip + pane stack | `src/components/sessions/SessionRuntime.tsx`, `SessionTerminalTabs.tsx` | rendering a tab and a body per conversation |
| The conversation | `src/components/sessions/useAgentConversation.ts`, `SessionAgentConversationPane.tsx` | `Open` / `Prompt` / `Cancel` for exactly one conversation |

Two pure modules carry the logic worth testing without a DOM:
`agentConversationTranscript.ts` (chunks → turns) and `agentConversationTabs.ts` (the tab list).

## Invariants

### 1. The runtime layer must keep one slot in the element tree

`SessionMainPane` renders `runtimeLayer` in the **same position** whether it is shown, hidden behind
an overlay (a workflow view, a dormant session's transcript), or absent because nothing is attached.

This is load-bearing. React unmounts a subtree that changes position, unmounting a runtime unmounts
the bodies of its open conversations, and **a body cancels its conversation as it unmounts**. Before
this was fixed, selecting a workflow session swapped the layer out for the workflow's own view, so an
operator who glanced at a PR-Stack session came back to tabs whose conversations the daemon had
already dropped — and the tabs, held in the screen's state, looked fine.

Selecting another session is not closing a tab. **Only closing a tab may end a conversation.**

A refactor that reintroduces a branch like `customView ? customView : runtimeLayer` reintroduces the
bug. `SessionAgentAttachTabAcceptance.cy.tsx` pins it with "keeps a conversation open when a workflow
session is selected instead".

**Known gap:** this holds for switching sessions, not for opening the create-session pane, which
skips the whole session-detail block. Surviving that too means hoisting the runtime layer above the
`PanelGroup`, which breaks the Code-pane split.

### 2. One owner for a conversation's whole life

The header mints the `conversation_id` and opens the tab. `useAgentConversation` — living in the tab's
body — issues `OpenAgentConversation` on mount and `CancelAgentConversation` on unmount.

The header must not also open it: that would open every conversation twice. And a failed open belongs
in the tab that holds it, not in a picker that has already closed.

The cleanup **waits for the open before cancelling**, and cancels only what opened. Cancelling while
the open is in flight lets the cancel land first (the daemon answers `NOT_FOUND` for a conversation it
has not created) and the open land after — leaving a conversation, and the agent session
`open_agent_conversation` spawns for it, with nothing left to cancel it.

### 3. One turn at a time, gated in `send()` and not on the button

`appendAnswerChunk` extends *the open agent turn*. A second prompt sent into an answer still arriving
therefore appends an operator turn after the incomplete agent turn, which makes the first stream's
next chunk open a fresh turn that the second stream then extends — two answers merged into one, with a
prompt stranded mid-answer, and whichever stream ends first clearing `answering`.

The gate lives in `send()` because the Send button is not the only way in; Enter is. `disabled`
reflects the gate, it is not the gate.

### 4. A conversation carries its own host

`AgentConversation.daemonInstanceId` is stamped at attach time, when it is known for certain, rather
than re-derived per render from the session list. An empty `daemon_instance_id` is not "unknown" on
the wire — it means "whichever daemon this request reached" — so a session briefly absent from the
list would silently route the prompt to the wrong host.

### 5. The id is minted with `randomUuid`

Never `crypto.randomUUID`: tddy-web is routinely served from a plain-http LAN origin where it is
`undefined`. See [insecure-origin-constraints.md](insecure-origin-constraints.md).
`OpenAgentConversation` accepts a caller-chosen id precisely so the caller can name — and therefore
cancel — what it opened.

## One picker, two mounts

`AgentPicker.tsx` serves both the Inspector's roster pane and the session header. It takes an
**explicit `testIdPrefix`** with no default (`agent-roster-picker` / `session-agent-picker`), because
both mounts can be on screen at once and a shared default would make each one's selectors match the
other's controls.

Its catalog is a fan-out over `useAvailableAgents`: `ListSubagents` carries no routing field and a
daemon answers only for its own defs, so the picker reads its home host through the browser's own
transport and addresses every other common-room daemon over LiveKit RPC. That home is **not** the
session's facilitating host — see the note in `useAvailableAgents.ts`.

## Attaching twice

`AttachSessionAgent` is a no-op on the roster the second time. `withAgentConversation` therefore
returns the list unchanged when the agent already has a conversation, and the existing tab is focused
— growing a second tab would claim something the daemon did not do.

Concurrent attaches of the same agent are prevented in `AgentPicker` rather than reconciled after the
fact: the attach resolves against state captured before its own `await`, so two in flight at once both
find no existing conversation, and the loser would focus an id the list never kept — no tab selected,
no pane rendered.

## Testing

- `cypress/support/rpc/agentConversationBackend.ts` — the fake. `PromptAgentConversation` is a real
  generator yielding one frame per chunk with only the last marked `last`, which is what makes
  "folds three frames into one turn" a test rather than a fixture readback. `holdAnswer` /
  `releaseAnswer` open a window *during* a turn; `failNextPrompt` fails one prompt without failing the
  ones already answered.
- `cypress/support/pages/sessionAgentConversationPage.ts` — picker, tabs and transcript helpers.
  `promptWithEnter` exists because Enter and the Send button are different ways in and only one of
  them is closed by `disabled`.
