# Managed-Codebase Mode + Discovery Subagents (ACP-shaped MCP)

## Summary

Formalizes today's "remote codebase" concept as a named session mode — **managed codebase** —
and adds the ability to wire **specialized subagents** into that mode. A subagent is a
conversational helper (starting with the existing FastContext discovery agent,
[discovery-agent.md](discovery-agent.md)) that the main coding agent (Claude Code) can open a
**conversation thread** to, over MCP, and ping-pong codebase questions against — instead of doing
every exploration step itself.

Managed-codebase mode is the *existing* behavior, renamed for users: the real codebase lives on a
daemon/host the agent cannot touch directly; every file/shell operation is proxied through
`mcp__tddy-tools__*` tools (see [remote-codebase-mode.md](../daemon/remote-codebase-mode.md)). This
feature does not change that proxying — it adds a second, independent MCP surface on the same
`tddy-tools --mcp` server: a small set of **subagent tools** shaped after **ACP**
(Agent Client Protocol) terminology, so opening/continuing a subagent conversation feels like the
same `session/new` → `session/prompt` shape the codebase already uses for `ClaudeAcpBackend` /
`CodexAcpBackend` ([codex-acp-backend.md](codex-acp-backend.md)).

## ACP → MCP tool mapping

| ACP concept (`agent-client-protocol` crate)      | MCP tool on `tddy-tools --mcp`                                     |
|---------------------------------------------------|----------------------------------------------------------------------|
| `session/new` (`NewSessionRequest`)               | `subagent_new_session` — input `{ agent?, sessionId?, cwd? }` → `{ sessionId }` |
| Client-chosen `SessionId`                         | `sessionId` input — the **main agent** decides the conversation id; a fresh id is generated only when omitted |
| `session/prompt` (`PromptRequest`)                | `subagent_prompt` — input `{ sessionId, prompt: [ContentBlock], graceMs?, maxTurns? }` → the turn's outcome, or `{ responseId, pending: true }` once `graceMs` elapses |
| *(no ACP counterpart)*                            | `subagent_resume` — input `{ sessionId, fromMessageId?, correction?, replacement?, maxTurns?, graceMs? }` → the same outcome shape, for a turn that resumes a yielded conversation with the caller's replacement call and result |
| *(no ACP counterpart)*                            | `subagent_await` — input `{ responseId, timeoutMs? }` → the same outcome, or `{ responseId, pending: true }` again |
| `PromptResponse.stopReason`                       | output field `stopReason`: `"end_turn"` \| `"max_turn_requests"` \| `"cancelled"` \| `"context_exhausted"` |
| Response `content` (`ContentBlock[]`)             | output field `content`: `[{ "type": "text", "text": "..." }]` |
| *(no ACP counterpart)*                            | output field `messages` — the messages **this turn appended**, each `{ id, role, tool, toolCalls, isError, preview, resultSummary }`; plus `clampedMaxTurns` when the ceiling cut the budget |
| `session/cancel`                                  | `subagent_cancel` — input `{ sessionId }` |

These tools use plain JSON (serde), not the `agent-client-protocol` crate — that crate's
`Client`/`Agent` JSON-RPC machinery drives a *subprocess*; here the "subagent" runs as an in-process
loop inside `tddy-tools`, exposed as ordinary MCP tools. Only the vocabulary (`sessionId`,
`stopReason`, `end_turn`, `ContentBlock`) is mirrored, so a reader familiar with ACP recognizes the
shape immediately.

"Yield when there's an opportunity for an extra prompt" (the requirement that the subagent's
internal tool-call ↔ tool-result loop hands control back to the main agent) = the loop terminating
on a `<final_answer>` (→ `stopReason: "end_turn"`), on hitting the turn budget in force for the call
(→ `stopReason: "max_turn_requests"`), or on a model that refuses the turn because its context
window is full (→ `stopReason: "context_exhausted"`, whose outcome carries a handoff brief for a
fresh conversation — that condition is not one the same conversation recovers from, because every
further turn re-sends the same oversized history). Exposed per-turn rather than only at the very end
of a whole invocation.

The one way a turn ends without a `stopReason` is **[a total tool outage](#a-turn-in-which-no-tool-call-succeeded-is-an-error)**,
which is an error result rather than a landing.

## Architecture

```
Claude Code (main agent, managed-codebase mode: native FS tools excluded)
   │  MCP (rmcp, stdio)
   ▼
tddy-tools --mcp  (PermissionServer)
   ├── exec tools  Read/Write/Shell/…  ──► dispatch_session_tool ──► daemon ExecuteTool (real worktree)
   └── subagent tools (NEW):
          subagent_new_session / subagent_prompt / subagent_cancel
              │  PermissionServer holds sessionId → Box<dyn SubagentSession>
              ▼
          tddy_discovery::subagent::SubagentRegistry  →  FastContextSession
              │  internal READ/GLOB/GREP tool-call ↔ tool-result loop
              ▼
          tddy_discovery::subagent::CodebaseAccess
              ├── Local    — direct host filesystem (co-located subagent)
              └── Managed  — injected dispatch fn (session_tool_client::dispatch_session_tool)
```

`CodebaseAccess` lets the *same* `FastContextSession` either read the host filesystem directly
(when the subagent runs on the same host as the real worktree — e.g. `tddy-sandbox-app` without
`--remote-codebase`) or read through the exact same proxy the main agent's exec tools use (when the
codebase is only reachable via the daemon). `tddy-discovery` never depends on `tddy-tools`; the
managed dispatch function is injected by the caller (`tddy-tools`) as a boxed async closure, keeping
the dependency direction `tddy-tools → tddy-discovery` (already true today via `FastContextBackend`
in `tddy-coder`) and never the reverse.

## User Story

As a developer running Claude Code in managed-codebase mode, I want to hand codebase-discovery
questions to a lightweight local-model subagent — instead of spending the main agent's own
tool-call budget exploring the repo — and keep talking to that same subagent across multiple
questions in one session, so that discovery stays fast and cheap without giving the main agent
direct filesystem access.

## Tool replacement (subagent-declared)

Wiring in a subagent is additive-only today: the three `subagent_*` tools are added, but the main
agent keeps its full exec-tool set, so nothing steers it toward actually using the subagent instead
of grepping/globbing the codebase itself.

A subagent can declare the exec tools it **replaces** (FastContext replaces `Grep`/`Glob` — its own
internal READ/GLOB/GREP loop already covers that ground). When a subagent with a non-empty replaced
set is wired in:

- **Enforcement (hard):** the replaced tools are dropped from the sandboxed Claude CLI's
  `--allowedTools` before the `mcp__tddy-tools__` prefix is applied — a direct call to a replaced
  tool is impossible, not merely discouraged.
- **Guidance (soft):** the managed-codebase appendix in CLAUDE.md/AGENTS.md is rendered to say those
  tools are unavailable and name the subagent that must be used instead.

The declared set has a per-subagent default (`tddy_discovery::subagent_replaced_tools`), carried as
`TDDY_SUBAGENT_REPLACES` into the jail. There is no caller-facing override for it (nor for a
subagent's `model`/`base_url`/`max_turns`) — all of it comes exclusively from the resolved agent's
YAML def (or the builtin `fastcontext` def); the earlier `--fastcontext-url`/`--fastcontext-model`/
`--fastcontext-max-turns`/`--subagent-replaces` flags and their `StartSessionRequest`/
`SessionMetadata` equivalents were removed (see criterion 24).

## Long turns: a grace period, a response id, and `subagent_await`

`subagent_prompt` blocks until the subagent yields. That is the right shape for the calls it was
designed around — a discovery question a 7B model answers in seconds — and the wrong one for what
the roster made addressable: a full coding agent on another host, whose turn is measured in minutes.
The main agent has one tool call in flight and no way to abandon it, so a long turn is
indistinguishable from a hung one, and a client-side deadline anywhere along the path (Claude Code's
own tool timeout, an MCP transport, the LiveKit RPC engine) ends the call while the turn keeps
running — the work is done, the answer is unreachable, and nothing says so.

The fix is to bound the *wait*, not the turn.

**A grace period.** `subagent_prompt` blocks for up to `graceMs` (default
`SUBAGENT_PROMPT_GRACE`, 25 s). A turn that yields inside it returns exactly what it returns today —
`{stopReason, content, usage}`, no new fields — so every fast call, and every existing caller of
one, is untouched. This is deliberately not a timeout: nothing is cancelled at the threshold.

**A response id.** A turn still running at the threshold returns `{responseId, pending: true}`. The
turn continues in the background; its outcome is stored under that id in the same process-wide table
the conversations live in.

**`subagent_await`.** `{responseId, timeoutMs?}` blocks until that turn's outcome is ready or its own
timeout elapses. Ready returns the outcome verbatim — the identical shape `subagent_prompt` would
have returned had it been fast. Not ready returns `{responseId, pending: true}` again, and the agent
calls it again. A polling loop the agent drives, with no deadline of its own to trip over.

### Why these choices

**The dual return shape, rather than always returning a `responseId`.** A single shape would be
tidier, and would break every caller of the fast path and cost every discovery question a second
round trip. The shapes are distinguishable by a key that only ever appears on one of them
(`pending`), and the tool description names both.

**`graceMs` per call, not an environment variable.** How long a caller can afford to block is the
caller's fact, not the host's: a workflow step waiting on a coding agent and a discovery question
mid-edit want different numbers, and both run in the same process.

**Queueing, not refusing, a prompt that arrives mid-turn.** A conversation's history is a single
sequence; two turns running against it concurrently would interleave into it. The second prompt
therefore waits for the first to finish and then runs — and its own `graceMs` covers that wait, so
the queue costs the caller nothing beyond the blocking budget it already named.

**A result stays claimable.** `subagent_await` does not consume the outcome; repeated calls on the
same `responseId` return the same answer. A tool result lost between this process and the main
agent — a truncated transport frame, a compaction, a restarted client — is otherwise an answer that
was computed, paid for, and permanently unreachable.

**Cancelling resolves what is pending.** `subagent_cancel`, and a detach that cancels a conversation
underneath it, close the conversation immediately as they do today, and resolve every pending
response of that conversation as cancelled. An `await` for a turn nobody will ever finish has to
answer, not wait.

### What does not change

The daemon. A remote agent's turn already arrives as a server stream
(`PromptAgentConversation`), consumed to completion by `AgentConversationLink::prompt`; the
background task drives exactly that call. Nothing about the wire, the frame contract, or the
truncation rule moves — only which task is awaiting it.

## Turn control: the budget, the transcript, and the resume

A subagent's turn budget, the record of what it did with the budget, and the ability to send it back
and try again are one surface. They exist because of the failure that has all three missing:
incident 2026-09-26, in which a discovery agent spent ten turns on tool calls that were every one
refused and answered with an invented file, line number and constant value — twice, byte for byte,
because turns run at `temperature: 0.0`.

### `maxTurns` — the budget belongs to the call

`subagent_prompt` and `subagent_resume` each accept an optional `maxTurns`: how many model turns
**that one call** may spend, in place of the agent definition's own budget. Omitted, the
definition's budget applies untouched.

- The caller's figure is bounded to `1..=50`. Outside it, the value is **clamped to the nearer
  bound and the outcome carries `clampedMaxTurns`** saying what was actually applied. Clamping
  rather than refusing keeps an over-eager caller working; reporting it is what stops a caller
  reading an early stop as a finished search.
- **A definition's own budget is never clamped.** It is the operator's configuration, and the
  ceiling exists to bound what a *caller* may spend, not to override a deliberate setting nobody in
  the call asked to change.
- The floor matters as much as the ceiling, and for a sharper reason. The loop runs `0..turns`, so a
  budget of zero runs no turn: nothing is read, no tool call fails, and the total-outage guard —
  which fires on a failure having happened — has nothing to fire on. Control would then reach the
  synthesis turn with a history holding only the prompt, which is the 2026-09-26 fabrication reached
  through a caller-controlled field with no tool outage anywhere.
- A roster entry still carries no turn budget of its own, and editing an agent definition still
  cannot change what a running session may do. The ceiling is what keeps that guarantee true in
  substance: a caller may ask for a longer search, not for an unbounded one.

### Every turn outcome enumerates the messages it appended

Alongside `{stopReason, content, usage}`, a turn outcome carries `messages` — the messages **that
turn appended**, in the order they happened, not the whole history:

```json
"messages": [
  {"id": "m18", "role": "assistant", "tool": null, "toolCalls": ["Read"], "isError": false, "preview": "I'll read the theme interface…"},
  {"id": "m19", "role": "tool", "tool": "Read", "toolCalls": [], "isError": true, "preview": "…its channel is closed",
   "resultSummary": {"error": true}}
]
```

- `id` is opaque and unique for the life of the conversation. Ids come from a counter that only
  rises, **including across a rewind**: an id a rewind discarded is never minted again, so an id
  held from before a rewind addresses nothing rather than silently addressing a different message.
- `role` is `system`, `user`, `assistant` or `tool`. A spelling the reader does not model is refused
  naming it rather than defaulted — describing a message as the wrong speaker misreports who said
  it.
- `isError` is the field whose absence let the incident run: a main agent could see how many
  messages went by, but not that every one of them was a refusal.
- `preview` is a handle, not a copy. It is cut to 240 characters, because one `Read` result in the
  incident was 42 KB and a turn outcome carrying a handful of those would put the agent's context
  back into its caller's — the cost the agent exists to avoid.
- `resultSummary` is the structured facts of a `tool`-role message's result, extracted from the
  result JSON at the moment it is appended and serialized as one externally tagged object —
  `{"read": {…}}`, `{"strReplace": {…}}` — so the caller dispatches on the same name it dispatches
  the tool call on. Per tool: `READ` reports `{firstLine, charsRead, totalLines, truncated}`
  (`firstLine` is the first non-empty line, cut to 120 chars — the only text a summary carries);
  `GREP`/`GLOB` report their match/path counts and totals (a `GREP` asked for `before`/`after`
  context also carries its matches' context lines — see below); `STR_REPLACE` reports
  `{replaced, matchedLines, bytesWritten}`; `SHELL` reports the exit code and output size, or the
  background job's id; `AWAIT`, `WRITE`, `DELETE` and `READ_LINTS` report their ending. A dispatch
  that produced no result — a failure, a rejection, a repeat — summarizes as `{"error": true}`,
  with `isError` remaining the authoritative flag. Every summary is bounded, so a descriptor's
  added byte cost on the final LiveKit frame is capped (~600 bytes per tool-role message, on top
  of the ~960-byte preview bound).

### Grep answers with the lines around each match

A `GREP` call may ask for `before`/`after` context (each 0–50 lines, either alone or together):
every match entry then carries a bounded `context` list — `{lineNumber, text, relation}` each,
`relation` spelled `before` or `after`, in file order — computed from the same source the match
ran against, so the signature-above/body-below question costs one call instead of a `READ` of the
whole file. A line two matches' windows share is attached to exactly one entry (the next match's
`before`, or the last match's `after`), and windows clamp at file edges — fewer lines than asked,
never padding and never an error. Without the arguments the result is byte for byte the
context-free shape, and the window semantics do not move: `truncated`/`total_matches` keep
counting **matches**, so context lines never consume the window. Both codebase paths answer
identically — the managed path folds ripgrep's own `context` events, the local path computes the
same windows from the file's lines.

### A caller's condition on a tool call yields the turn back

A `subagent_prompt` or `subagent_resume` call may carry **`yieldConditions`** — up to 8 conditions,
each naming a tool and what to watch for on its call: an **`outcome`** fact compared for equality
against the call's result summary (`{matchedLines: 0}`, `{exitCode: 1}`, `{matchCount: 0}`,
`{error: true}` — the same vocabulary the result summary reports), or an **`argument`** string
field containing a bounded substring (≤256 chars). Conditions are per-turn-request state: they
apply to this turn only, never the conversation.

When a condition matches, the turn stops **at that call**: the tool result just appended stays in
the transcript, the model is never sent it and takes no further step, and the outcome returns
`stopReason: "yieldedToCaller"` naming the fired condition and the tool message's id — the id
`subagent_resume` consumes. Malformed conditions are rejected before the turn runs, each naming
the offending condition: an unknown tool, a fact that tool's summary cannot carry (a
`matchedLines` condition on a `WRITE` is a refusal, never a condition that silently never fires),
an over-long needle, or too many conditions.

### `subagent_resume` — carry on, or go back and correct

`subagent_resume { sessionId, fromMessageId?, correction?, resetWorktree?, maxTurns?, graceMs? }` takes another turn
on an open conversation and **sends no new prompt turn**.

- With neither `fromMessageId` nor `correction` it continues the history as it stands, under a fresh
  budget — the case where a chain was cut short by its budget and re-asking would make the agent
  re-read everything it has already read.
- `fromMessageId` sends the conversation back to that message, discarding everything after it. A
  rewind landing between a tool call and its results **keeps the results**, so the history stays one
  the model accepts. An id the conversation does not hold is an error naming it, never a silent
  continue; so is an unknown `sessionId`. When the conversation has a worktree, the rewind takes its
  files back too — see [A rewind takes the worktree back](#a-rewind-takes-the-worktree-back).
- `correction` appends exactly one corrective instruction after the rewind point. It is what makes a
  rewind able to change anything at all: turns go out at `temperature: 0.0`, so re-sending identical
  context reproduces an identical turn. A rewind offered without a correction would be a feature
  that silently does nothing.
- Present-but-empty is refused for both fields rather than treated as absent, for the same reason
  `subagent_prompt` refuses an empty prompt: a caller that sent a field meant to send something in
  it.
- A resume may carry a **`replacement`** — a `{tool, arguments, result}` the caller substitutes
  for the call that yielded: appended after any rewind and correction as an assistant tool-call
  message plus its tool result, with minted ids, and the turn continues over the appended
  history. The original failed call **stays** in the history (append-only: nothing is rewritten),
  the appended call **never dispatches** — the result is the caller's text, recorded verbatim —
  and a malformed replacement is refused before the turn runs, above the rewind, so the refusal
  never reshapes the history it is refused from.
- A resume queues exactly as a prompt does — a conversation runs one turn at a time — and reports
  `queuePosition` / `queueSize`, `graceMs` and `responseId` identically.

### A turn in which no tool call succeeded is an error

When a call's whole budget goes on tool calls of which **none ran**, the turn returns an error
naming the transport failure, and returns it **before any model call**. The guarantee is that
nothing was asked to summarise, not that its answer was discarded afterwards: the synthesis turn
asks for "the specific file:line locations you found" without checking that anything was found, and
a model at `temperature: 0.0` obliges with an invented one.

The conversation is left exactly as the failed call built it and stays promptable. Its history is
the record of what was attempted, so a caller that has fixed the tool channel can carry on from
there, and `subagent_resume` can rewind into it.

A tool that **ran** and exited non-zero is not this condition. One failed call among successful ones
is ordinary traffic, and a call in which some tools worked keeps today's `max_turn_requests` soft
landing unchanged.

**The guard fires at budget exhaustion, and there alone.** A model that ends the turn early with a
final answer after every tool call failed still returns that answer, because the loop never reaches
the exhaustion path where the tally is read — recorded in
[`docs/dev/todo/2026-09-26-the-honest-failure-guard-does-not-cover-a-model-ended-turn.md`](../../dev/todo/2026-09-26-the-honest-failure-guard-does-not-cover-a-model-ended-turn.md).

### What is proven, and where the proof stops

Every conversation in a jailed deployment is a **remote** one: the jail holds no agent definition, so
the turn loop runs on the facilitating daemon and each of these fields crosses
`session_agents.SessionAgentService` (see
[session-agent-roster.md](../daemon/session-agent-roster.md)). That crossing is compile-checked and
covered on either side — `tddy-discovery`'s local-session tests for the loop, the `--mcp` stdio
acceptance tests for the wire shape — but **no test drives `ResumeAgentConversation` end to end**:
the suite that would runs on no CI machine. Recorded in
[`docs/dev/todo/2026-09-26-the-resume-rpc-and-its-turn-budget-have-no-wire-level-test.md`](../../dev/todo/2026-09-26-the-resume-rpc-and-its-turn-budget-have-no-wire-level-test.md).

A second, narrower gap on the same path: a turn's `messages` list rides the final conversation chunk,
and a long one can overflow the chunk-framing threshold —
[`docs/dev/todo/2026-09-26-a-turns-message-list-can-overflow-the-chunk-framing-threshold.md`](../../dev/todo/2026-09-26-a-turns-message-list-can-overflow-the-chunk-framing-threshold.md).

## The conversation worktree: a subagent edits its own tree

A subagent whose definition binds mutation tools (`WRITE`, `STR_REPLACE`, `DELETE`, `SHELL`) does
not write its caller's worktree. Its edits land in a worktree of the **conversation's** own, one
commit per mutating call, and come back to the caller when the conversation ends. This covers the
in-process loop in `tddy-tools` running with Managed access; conversations whose loop the daemon
runs are a [non-goal](#non-goals-out-of-scope-for-v1).

- **Created lazily.** The conversation's first *mutating* call cuts branch
  `tddy/subagent/<session id>/<conversation id>` and worktree
  `<session worktree>/tmp/subagent-worktrees/<conversation id>` from the caller's `HEAD`. A
  conversation that only reads never creates one. The directory is excluded through the repository's
  `info/exclude`, so it never shows in the caller's `git status`.
- **Seeded with the caller's uncommitted state.** Staged, unstaged and untracked (not ignored)
  changes become **one commit** on the subagent branch; the caller's index, branch and files are left
  as they were. That commit, or `HEAD` when the caller was clean, is the conversation's **base**.
- **Every call of the conversation runs there once it exists**, reads included, so the subagent reads
  what it wrote. Before the first write, a read runs in the session worktree.
- **A conversation id** must be `[A-Za-z0-9._-]`, no leading `.`, at most 64 characters, and not
  break a git ref rule; otherwise it is refused before any git state is created.
- **One commit per mutating call that changed files**, its subject the tool's name, authored as
  `tddy-subagent`. A call is mutating unless it is `READ`, `GLOB`, `GREP`, `SEMANTIC_SEARCH` or
  `READ_LINTS`; the list is fail-closed, so `AWAIT` (a background `SHELL` job writes files while it
  is awaited) and any unknown tool are mutating. A call that changed nothing makes no commit.
- **`worktreeChange` on the tool result** of every mutating call, a sibling of `resultSummary`:

  ```json
  "worktreeChange": {
    "commit": "3f9c2ab",
    "files": { "created": 1, "updated": 2, "removed": 0 },
    "lines": { "added": 41, "removed": 7 }
  }
  ```

  `commit` is present only when a commit was made; a binary file counts as a file and adds no
  lines; a read carries none. It is bounded — counts and a hash, never paths.
- **`subagent_end { sessionId }`** applies everything committed since the base to the caller's
  worktree as **uncommitted changes**, 3-way: where the caller changed the same lines since, the file
  is written with conflict markers. It then deletes the worktree and branch and closes the
  conversation, and answers
  `{"ended": true, "pulled": {"files": {...}, "lines": {...}, "conflicts": ["src/lib.rs"]}}` —
  `pulled` is `null` when the conversation never created a worktree. The caller's `HEAD` never moves
  and no commit is made on the caller's branch. It is refused while a turn is running, naming the
  turn; a failed pull leaves the conversation open so the caller can retry or cancel.
- **`subagent_cancel`** deletes the worktree and branch; nothing reaches the caller.

A subagent's edits therefore reach the caller only on `subagent_end`; a conversation that is
cancelled loses them, which is the point, and the tool descriptions say so.

Git runs on the facilitating daemon's host, never in the jail: a linked worktree's `.git` points into
the repository's common directory, which a jail mounting only the checkout cannot see. The jail
relays `ExecuteTool{conversation_id}` and the typed `ConversationWorktree` RPC (`Pull`, `Remove`) to
the host. The automatic commits skip hooks and signing — every git call runs with
`core.hooksPath=/dev/null` and `commit.gpgsign=false`, and the commit with `--no-verify` — so no
developer hook runs on a subagent's behalf and a signing prompt cannot block a call. That is an
implementation choice the developer has not confirmed; see
[`docs/dev/todo/2026-10-01-subagent-commits-skip-hooks-and-signing-without-consent.md`](../../dev/todo/2026-10-01-subagent-commits-skip-hooks-and-signing-without-consent.md).
Mechanics: [`tddy-subagent-worktree`](../../../packages/tddy-subagent-worktree/docs/conversation-worktree.md).

### A rewind takes the worktree back

`subagent_resume { fromMessageId }` rewinds the transcript; without more, the edits the dropped
messages made would stay in the worktree and the resumed subagent would read a tree its own history no
longer explains. So a rewind **also resets the conversation's worktree**, by default.

- **The target** is the commit of the **last entry the rewind keeps** that made one. The cut already
  extends over the tool results answering the named message, so a call's commit is kept with its call.
  When no kept entry made a commit, the target is the conversation's base.
- **The reset is hard.** The worktree and its branch move to the target, commits past it are dropped
  from the branch, and untracked files they created are removed. **Ignored files are kept**, so build
  output under the worktree survives a reset.
- **`resetWorktree: false`** opts out: the worktree and branch are left as they are, and later
  commits build on them. Absent or `true` resets. A non-boolean is refused by name.
- **The outcome reports it**, beside `messages`:

  ```json
  "worktreeReset": { "to": "3f9c2ab", "droppedCommits": ["9e01d4c", "a77b310"] }
  ```

  `droppedCommits` are short hashes, oldest first. The field is absent when nothing was reset: no
  rewind, `resetWorktree: false`, or a conversation that never made a mutating call (so has no
  worktree). A rewind on a conversation with no worktree resets nothing and creates nothing.
- **It happens before the resumed turn's first model call**, so the subagent's first read sees the
  reset tree.
- **A failed reset refuses the resume** before the transcript is rewound: the conversation is exactly
  as it was, and the error says why. A target that is not a commit of the conversation's branch is
  refused before anything moves.
- A caller's **`replacement`** is never dispatched, so it makes no commit; a reset to a point after a
  replacement lands on the last real commit.
- Nothing reaches the caller's worktree: the reset moves only the conversation's own tree, and
  `subagent_end` still hands over what the branch then holds.

Applies to the in-process loop with Managed access, like the rest of the worktree. Daemon-run
conversations have no worktree to reset.

### Known gaps

- A `tddy-tools` process that dies without ending or cancelling its conversations leaves the branch
  behind —
  [`docs/dev/todo/2026-09-30-an-abandoned-subagent-conversation-leaves-its-branch.md`](../../dev/todo/2026-09-30-an-abandoned-subagent-conversation-leaves-its-branch.md).
- Daemon-run conversations still write the session worktree —
  [`docs/dev/todo/2026-09-30-daemon-run-subagent-conversations-still-write-the-session-worktree.md`](../../dev/todo/2026-09-30-daemon-run-subagent-conversations-still-write-the-session-worktree.md).
- Over the jail host bridge the session a conversation request names is not checked host-side —
  [`docs/dev/todo/2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge.md`](../../dev/todo/2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge.md).
- `subagent_end` can race a prompt arriving between its pending check and the retire —
  [`docs/dev/todo/2026-10-01-subagent-end-races-a-prompt-and-ignores-a-failed-cancel.md`](../../dev/todo/2026-10-01-subagent-end-races-a-prompt-and-ignores-a-failed-cancel.md).
- There is no diff tool or range pull; a pull applies everything since the base as one diff. Those
  belong to the `#agent-worktree` stack's later PRs
  ([#562](https://github.com/uppin/tddy-coder/pull/562), [#563](https://github.com/uppin/tddy-coder/pull/563)).
  Until the range pull lands, `worktreeReset` cannot say which dropped commits had already been
  handed to the caller.

## Acceptance Criteria

### Subagent session lifecycle (`tddy-discovery`)

1. `SubagentRegistry::create("fastcontext", config)` returns a `Box<dyn SubagentSession>`; an
   unknown name returns a typed error, not a panic or a silent default.
2. A `FastContextSession` retains its message history across multiple `prompt()` calls — a second
   `prompt()` sees the model's and tool results from the first, matching a real multi-turn
   conversation rather than resetting each call.
3. `prompt()` returns `stop_reason: EndTurn` when the model produces a `<final_answer>`, and
   `stop_reason: MaxTurnRequests` when the configured per-prompt turn budget is exhausted with no
   `<final_answer>` — never panics, never loops forever.
4. `CodebaseAccess::Local` executes READ/GLOB/GREP against the local filesystem (same semantics as
   `ToolExecutor::Local`).
5. `CodebaseAccess::Managed` maps READ/GLOB/GREP to `Read`/`Glob`/`Grep` and dispatches them through
   an injected async function rather than `ToolExecutor::Remote`'s own HTTP client — the same
   function `tddy-tools` already uses for its exec-tool proxying
   (`session_tool_client::dispatch_session_tool`), so a managed subagent and the main agent's exec
   tools share one transport-detection path.

### MCP surface (`tddy-tools`)

6. With `TDDY_SUBAGENT=fastcontext` set, `tools/list` over the real MCP stdio wire includes
   `subagent_new_session`, `subagent_prompt`, and `subagent_cancel`; without it, none of the three
   are present.
7. `subagent_new_session` with a caller-supplied `sessionId` uses that exact id — the main agent, not
   the subagent server, decides the conversation id (matching the plan's "main agent decides the
   conversation ID" requirement).
8. `subagent_prompt` against a `sessionId` opened by `subagent_new_session` returns
   `{ stopReason, content }`; a second `subagent_prompt` call against the same `sessionId` continues
   the same conversation (criterion 2, exercised end-to-end over the MCP wire).
9. `subagent_prompt` against an unknown `sessionId` returns an error result (`is_error:true`), not a
   panic and not a silently-created new session.

### Allowlist (`tddy-sandbox-recipes`)

10. The sandboxed Claude CLI `--allowedTools` list includes
    `mcp__tddy-tools__subagent_new_session`, `mcp__tddy-tools__subagent_prompt`, and
    `mcp__tddy-tools__subagent_cancel` when a discovery subagent is enabled for the session, and
    omits all three when it is not.

### `tddy-sandbox-app` spawn wiring

11. `--codebase-mode managed` is accepted and is equivalent to today's `--remote-codebase`; the
    latter remains a working (deprecated) alias.
12. `--specialized-agent fastcontext` is threaded into the spawned sandbox's environment so the
    in-jail `tddy-tools --mcp` process constructs a `fastcontext` subagent on demand, using that
    def's own YAML-declared `base_url`/`model`/`max_turns` (see criterion 24 — there is no
    caller-facing override).

### Tool replacement (`tddy-discovery`, `tddy-sandbox`, `tddy-sandbox-recipes`, `tddy-sandbox-app`)

13. `tddy_discovery::subagent_replaced_tools("fastcontext")` returns `["Grep", "Glob"]`; an unknown
    subagent name returns an empty set (no panic, no fabricated tool name).
14. `tddy_discovery::resolve_replaced_tools(name, override_csv)` returns the declared default when
    `override_csv` is `None` or empty, and the override's tool names (normalized to the exec
    catalog's casing) when non-empty — the override always wins over the default, never merges with
    it. A token that doesn't match a known exec tool is dropped rather than passed through.
15. `tddy_sandbox_recipes::build_claude_allowlist(subagent_enabled, replaced)` omits
    `mcp__tddy-tools__<Tool>` for every `Tool` in `replaced`, while every other exec tool from
    `tddy_sandbox::workspace_exec_tool_names()` (plus `AskUserQuestion`, plus the subagent tools when
    `subagent_enabled`) is still present. An empty `replaced` slice reproduces today's full-exec
    allowlist exactly (no regression for sessions without a replacing subagent).
16. `tddy_sandbox::context_dir::sandbox_remote_appendix(subagent, replaced)` — when `replaced` is
    non-empty — states that those tools are not available as direct tools and names the subagent
    that must be used for them, in addition to (not instead of) listing the still-available exec
    tools. When `replaced` is empty, the rendered text is unchanged from today's appendix.
17. ~~`tddy-sandbox-app`'s `subagent_env_overlay` sets `TDDY_SUBAGENT_REPLACES` only when an
    explicit override is given~~ — superseded by criterion 24: there is no override anymore.
    `TDDY_SUBAGENT_REPLACES` always carries the resolved def's own declared `replaces`
    (normalized), set unconditionally whenever exactly one agent is wired.
18. ~~`tddy-daemon`'s own sandboxed-session path threads `StartSessionRequest`'s
    `fastcontext_url`/`fastcontext_model`/`fastcontext_max_turns`/`subagent_replaces` fields~~ —
    those fields were removed (criterion 24), along with the `discovery_subagent` field itself
    (criterion 24) and its parallel `TDDY_SUBAGENT_FASTCONTEXT_*` env mechanism. `specialized_agents`
    (even a single-element array) is the only wiring path, for both new-session start and resume.

### Tool replacement, generalized to the specialized-agent array

[specialized-subagents.md](specialized-subagents.md) generalized subagent wiring from one hardcoded
name to an array of YAML-defined `SpecializedAgentDef`s, but shipped with no tool-replacement
wiring for that array path — only the single-name `discovery_subagent` path (criteria 13-18 above,
since removed) enforced/rendered replaced tools. The following criteria connect the two: every
specialized agent in the array can declare its own replaced-tool set, and `tddy-sandbox-app` is
fully migrated onto the array model.

19. `SpecializedAgentDef` gains a `replaces: Vec<String>` field (`#[serde(default)]` — absent in
    YAML defaults to `[]`, replacing nothing). `builtin_fastcontext_def()` sets
    `replaces: ["Grep", "Glob"]`, matching criterion 13's single-name default exactly (single
    source of truth — `tddy_discovery::subagent::subagent_replaced_tools("fastcontext")` now derives
    from this field rather than a separate hardcoded literal).
20. `tddy_discovery::subagent::normalize_replaced_tools(tokens)` trims, case-insensitively matches
    against the canonical exec-tool catalog, canonicalizes casing, drops unrecognized tokens, and
    de-duplicates preserving first-occurrence order.
    `resolve_replaced_tools_for_defs(defs: &[SpecializedAgentDef])` unions every def's own
    `replaces` list through that same normalization — the array-model counterpart to criterion 14's
    single-name `resolve_replaced_tools`.
21. `tddy_sandbox::context_dir::sandbox_remote_appendix(replacements: &[SubagentReplacement])`
    accepts one entry per active agent (`SubagentReplacement { name, replaced }`) and renders a
    per-agent breakdown — each agent named next to the tools it specifically replaces, not a single
    flattened list — plus an "pass `agent: "<name>"` to select which subagent" hint when more than
    one agent is active. An empty `replacements` slice (or one where every entry's `replaced` is
    empty) reproduces today's unchanged appendix (no regression for sessions without a replacing
    subagent). `SandboxContextDir::create_with_subagent` takes the same `&[SubagentReplacement]`
    slice, replacing its old single `(subagent: Option<&str>, replaced: &[&str])` signature.
22. `tddy-daemon` has a single wiring path: `specialized_agents` (array — even a single-element
    array selects exactly one agent). `ConnectionServiceImpl::resolve_specialized_agent_defs`
    resolves `specialized_agents` names once per call (unknown name ⇒ `InvalidArgument`, naming the
    unresolvable agent); `specialized_subagent_env` builds the `TDDY_SUBAGENT`/`TDDY_SUBAGENTS_JSON`
    env pair from the already-resolved defs. The per-agent `SubagentReplacement` list feeds
    `prepare_context_dir_with_subagent` for both `start_sandboxed_claude_cli_session` and
    `relaunch_sandboxed_runner` (the latter takes a `specialized_agents: &[String]` parameter, so a
    resumed session re-resolves and re-wires the same defs it started with). There is no
    `discovery_subagent` field, and therefore no mutual-exclusivity concern to guard against.
23. `SessionMetadata` has a `specialized_agents: Vec<String>` field
    (`#[serde(default, skip_serializing_if = "Vec::is_empty")]`) — omitted from `.session.yaml` when
    empty, defaults to empty for legacy files without the key. `resume_sandboxed_claude_cli_session`
    reads `meta.specialized_agents` and passes it straight through to `relaunch_sandboxed_runner`
    (criterion 22), so a resumed session's specialized-agent wiring survives a daemon restart. There
    is no `discovery_subagent` field on `SessionMetadata` to fold in.
24. `tddy-sandbox-app` is migrated off the single-subagent-only flag set onto the array model:
    `--specialized-agent <name>` (repeatable) + `--agents-dir` (default `<session-base>/agents`) is
    the only way to wire a subagent in — **no backwards compatibility was retained**.
    `--discovery-subagent` (the deprecated single-name alias) was removed entirely, not merely
    deprecated: from `tddy-sandbox-app`'s CLI, from `StartSessionRequest`'s proto field 19, from
    `SessionMetadata`, and from `tddy-daemon`'s request handling — a caller wanting exactly one agent
    passes a single-element `specialized_agents`/`--specialized-agent` value. **The legacy
    `--fastcontext-url`/`--fastcontext-model`/`--fastcontext-max-turns`/`--subagent-replaces`
    override flags were also removed entirely** (from `tddy-sandbox-app`'s CLI and
    `SubagentSpawnConfig`, from `StartSessionRequest`'s proto fields 20-23 — now `reserved` — and
    daemon threading, and from `SessionMetadata`): every specialized agent's configuration comes
    exclusively from its resolved YAML def (or the builtin `fastcontext` def), with no
    caller-facing override at any layer. `spawn::subagent_env_overlay(defs)` emits `TDDY_SUBAGENT`
    (comma names) + `TDDY_SUBAGENTS_JSON` (serialized defs) for any number of agents, plus
    `TDDY_SUBAGENT_REPLACES` in the single-agent case (always the def's own declared `replaces`) —
    the same env shape `tddy-sandbox-runner` and `tddy-tools --mcp` already consume via
    `TDDY_SUBAGENTS_JSON` (criterion 9). This closes out `docs/ft/coder/specialized-subagents.md`
    ACs 11-12 (previously tracked as unimplemented in `docs/dev/TODO.md`).

### Asynchronous prompt turns (`tddy-tools`)

25. `subagent_prompt` blocks for at most a grace threshold — `graceMs` when the call names one,
    otherwise `SUBAGENT_PROMPT_GRACE` (25 s). A turn that yields inside the threshold returns
    `{stopReason, content, usage}` with no `responseId` and no `pending` key: a fast call is
    byte-for-byte what it is today, and a caller written against that shape needs no change.
26. A turn still running at the threshold returns `{responseId, pending: true}` and is **not**
    cancelled — the turn continues in the background and its outcome is stored under `responseId`.
27. `subagent_await` takes `{responseId, timeoutMs?}` and blocks until that turn's outcome is ready
    or its own timeout elapses (`timeoutMs` when named, else the same 25 s default). Ready returns
    the outcome in the identical `{stopReason, content, usage}` shape `subagent_prompt` returns;
    not ready returns `{responseId, pending: true}`, so the agent retries rather than treating the
    turn as lost.
28. A turn that *fails* is stored as its failure: `subagent_await` returns the
    `{error, is_error: true}` result the synchronous call would have returned, rather than reporting
    a turn that will never produce an answer as forever pending.
29. Awaiting is non-destructive and repeatable: two `subagent_await` calls on the same `responseId`
    return the same outcome. An unknown `responseId` is an error naming it — never a hang.
30. A `subagent_prompt` that arrives while a turn is already running on that `sessionId` **queues**
    behind it and runs after it, in call order. The queued call's own grace threshold covers the
    wait, so a call that does not reach the front in time returns `{responseId, pending: true}` like
    any other slow turn.
31. `subagent_cancel` on a conversation with a turn in flight closes it immediately, as it does
    today, and resolves every pending response of that conversation as cancelled — so a later
    `subagent_await` on one of those ids answers instead of waiting for a turn nobody will finish.
    A conversation cancelled because its agent was detached resolves its pending responses the same
    way.
32. Token accounting is unchanged by *where* a turn ran: once a background turn ends, `subagent_list`
    and the accounting file report its usage and its increment to `turns`. While it is in flight they
    report the conversation's pre-turn totals — never a partial turn's.
33. The sandboxed Claude CLI's `--allowedTools` includes `mcp__tddy-tools__subagent_await` exactly
    when it includes `mcp__tddy-tools__subagent_prompt`. An agent that can be handed a `responseId`
    and cannot call `subagent_await` has been given a receipt it can never redeem.
34. The advertised `subagent_prompt` description states both return shapes and names
    `subagent_await` as what collects the pending one — the schema is the only place the main agent
    can learn that a `responseId` is not an error.

### Turn control (`tddy-discovery`, `tddy-tools`, `tddy-sandbox-recipes`)

35. `subagent_prompt` accepts `maxTurns` and honours it **for that call only**: a later call on the
    same conversation with no `maxTurns` runs on the agent definition's budget again.
36. A `maxTurns` outside `1..=50` is clamped to the nearer bound and the outcome carries
    `clampedMaxTurns` with the applied figure; a value inside the range is not reported as clamped.
    A definition's own budget above the ceiling stands unclamped.
37. A `maxTurns` that is not a whole number of turns is refused naming the field, never ignored.
38. Every turn outcome — `subagent_prompt`, `subagent_await`, `subagent_resume` — carries
    `messages`: the messages that turn appended, each with `id`, `role`, `tool`, `toolCalls`,
    `isError`, a `preview` **shorter than the payload it previews**, and — on a `tool`-role
    message — a bounded `resultSummary` naming the tool's facts (`read`, `strReplace`, `shell`,
    …; `{"error": true}` when the dispatch produced no result).
39. Message ids are unique for the life of the conversation, and an id discarded by a rewind is
    never minted again.
40. `subagent_resume` with `sessionId` alone continues the conversation and sends no new prompt
    turn; the next request the provider receives carries the existing history and no appended user
    message.
41. `subagent_resume { fromMessageId }` discards every message after that id, and the rewound
    history — compared element-wise, not by length — is what the next turn is sent. A rewind never
    separates a tool-call message from its results.
42. `subagent_resume { correction }` appends exactly one corrective user message, in final position
    after the rewind point.
43. An unknown `sessionId` or `fromMessageId`, and a present-but-empty `fromMessageId` or
    `correction`, are in-band error results naming the cause — never a silent continue and never a
    protocol error.
44. A call whose every tool call failed returns an error naming the transport failure and makes
    **no synthesis model call**: a budget of *N* causes the provider to receive exactly *N*
    requests, never *N*+1. A call in which some tool calls succeeded keeps the `max_turn_requests`
    landing unchanged, and one failed call among successful ones is not an outage.
45. A conversation whose call failed that way is still open and can be rewound into: a following
    `subagent_resume` sends a history that still contains the failed call's tool exchanges.
46. `subagent_resume` is advertised in the MCP catalog exactly where `subagent_prompt` is — gated on
    the same roster condition, with `fromMessageId`, `correction` and `maxTurns` in its schema —
    and is present in the sandboxed Claude CLI `--allowedTools` list exactly where
    `mcp__tddy-tools__subagent_prompt` is. The two lists are separate and hand-maintained, so a
    tool that reaches only one of them is advertised and uncallable — which reads to the main agent
    as an agent that is not registered.

### Conversation worktree (`tddy-subagent-worktree`, `tddy-discovery`, `tddy-tools`, `tddy-sandbox-recipes`, `tddy-session-lifecycle`)

47. A conversation that only reads never creates a worktree or a branch.
48. The first mutating call creates `<session worktree>/tmp/subagent-worktrees/<conv>` on
    `tddy/subagent/<session>/<conv>`, cut from the caller's `HEAD`; the caller's uncommitted changes,
    untracked files included, are one commit on that branch, and the caller's index, branch and files
    are unchanged. The worktree never appears in the caller's `git status`.
49. After creation, reads see the subagent's own writes.
50. Each mutating call that changed files makes exactly one commit; one that changed nothing makes
    none. `AWAIT` and unknown tools are mutating.
51. `worktreeChange` reports created / updated / removed files, added / removed lines, and the
    commit's short hash when one was made; a read carries none.
52. An unsafe conversation id is refused before any git state is created.
53. `subagent_end` applies base..tip to the caller's worktree as uncommitted changes, 3-way, with
    conflict markers and the conflicted paths reported; the caller's `HEAD` does not move. It deletes
    the worktree and branch and closes the conversation, and is refused while a turn runs.
54. `subagent_cancel` deletes the worktree and branch; the caller's worktree is unchanged.
55. `subagent_end` is advertised beside `subagent_cancel` and allowlisted in the sandbox recipes
    exactly where `subagent_cancel` is.
56. A rewind resets the worktree to the commit of the last kept entry that made one, and to the
    conversation base when none did; untracked files the dropped calls created are removed and
    ignored files are kept.
57. The reset happens before the resumed turn's first model request, and the outcome reports
    `worktreeReset { to, droppedCommits }` (short hashes, oldest first).
58. `resetWorktree: false`, a resume without a rewind, and a conversation without a worktree ask for
    no reset and report none; a rewind creates no worktree.
59. A failed reset refuses the resume and leaves the transcript un-rewound.
60. `subagent_resume` advertises `resetWorktree` as a boolean.

Verified at the request seams and against real git repositories; **no test runs a real jail end to
end** (this needs a sandbox the development host cannot start).

## Non-goals (out of scope for v1)

- Live catalog fetch of subagent tool schemas over the transport (mirrors the existing
  `exec_tool_catalog()` limitation — see remote-codebase-mode.md AC16).
- Streaming partial subagent output back to the main agent mid-turn. A turn still yields exactly
  once, as a whole answer; what "Long turns" below adds is a way to stop *waiting* for it, not a
  way to watch it arrive.
- ~~Subagents other than FastContext~~ — addressed by
  [specialized-subagents.md](specialized-subagents.md): the registry now resolves any number of
  YAML-defined agents (`<tddyhome>/agents/*.yaml`), not just the hardcoded `"fastcontext"` factory.
- Renaming the internal `RemoteToolEnv` / `TDDY_REMOTE_*` wire vocabulary to "managed" — only the
  user-facing surface (CLI flags, help text, context-dir appendix prose, docs) is renamed; the
  daemon/sandbox IPC wire and its tests are left alone.
- ~~A UI/CLI picker for choosing which subagents to wire~~ — addressed by
  [specialized-subagents.md](specialized-subagents.md) for the daemon-driven web UI (a collapsible
  "Managed codebase" multi-select in session creation); the standalone `tddy-sandbox-app` CLI
  picker remains flag-driven only (tracked in `docs/dev/TODO.md`).
- Extending tool-replacement enforcement to the `tddy-coder --remote` path (that path does not wire
  subagents at all today — see `docs/dev/TODO.md`).
- Per-tool replacement policies beyond a flat replaced-set (e.g. partial replacement of `Grep` for
  some file types only).
- **Daemon-run conversations** (`open_local`, `open_owned`, a peer's `RemoteAgentSession`) have no
  conversation worktree; their calls still run on the session worktree.
- **Sweeping orphaned conversation worktrees and branches.**
- **Resetting the worktree on a rewind, diffing, range pulls** — later PRs of the `#agent-worktree`
  stack.

## Standalone launcher (`./claude-sandbox`)

A one-command launcher wraps `tddy-sandbox-app` for the common case of running a sandboxed Claude
Code session against the **current directory as a managed (unmounted) repo**, with specialized
subagents wired in from a single YAML config. It lives at the tddy-coder repo root as
`./claude-sandbox` and is invokable from any CWD (symlink-safe root resolution).

```bash
cd ~/my/project
claude-sandbox -c ~/sandbox-config.yaml -- "implement the login form"
```

### What the launcher does

- Resolves the tddy-coder repo root from its own real location (symlink-safe), so it works whether
  invoked by absolute path or via a PATH symlink from any directory.
- Passes `$PWD` as `--repo` (the managed repo the sandbox's `mcp__tddy-tools__*` calls operate on).
- Resolves the host `claude` binary to an absolute path — the jail's `PATH` is only
  `/usr/bin:/bin`, so a bare name (or a wrapper shim that re-execs `claude` from `PATH`, e.g.
  Superset's `~/.superset/bin/claude`) would fail to resolve inside the jail. `resolve_claude()`
  prefers `~/.local/bin/claude`, then scans `PATH` skipping `*/.superset*/bin` dirs; override with
  `--claude-binary /path/to/claude`.
- Builds `tddy-sandbox-app` + `tddy-tools` + `tddy-sandbox-runner` via
  `nix develop "$ROOT" --profile "$ROOT/.nix-profile" -c cargo build` into one target dir (they
  must sit as siblings — `tddy-sandbox-app` resolves the other two as siblings of its own
  executable). `--release` switches to the release profile; `--no-build` skips the build step.
- Execs the built `tddy-sandbox-app` binary on the host so it inherits `claude` on `PATH`.

### Flags

| Flag | Purpose |
|------|---------|
| `-c` / `--config <yaml>` | `SandboxAppConfig` YAML (see below); CLI flags override config values |
| `--release` | Build with the release profile (default: debug) |
| `--no-build` | Skip the cargo build step (assumes binaries already built) |
| `--claude-binary <path>` | Override the `claude` binary resolution |
| Any other flag | Forwarded to `tddy-sandbox-app` |
| `-- <args>` | Forwarded verbatim to the in-jail `claude` (after fixed flags + MCP allowlist, before the MCP args — a trailing positional prompt therefore lands last) |

### YAML config (`sandbox-config.example.yaml`)

A starter config lives at the repo root. Schema: `packages/tddy-sandbox-app/src/config.rs`
(`SandboxAppConfig`, `deny_unknown_fields`). Every field is optional; equivalent CLI flags override
config values. The config carries:

- `model`, `permission_mode`, `codebase_mode` (`managed` | `mounted`).
- `subagents:` — a list of full inline `SpecializedAgentDef`s (same schema as
  `<tddyhome>/agents/*.yaml`). Declaring one here **both defines and activates** it and overrides a
  same-named builtin, so e.g. `fastcontext` can be re-pointed at a local Ollama server
  (`base_url: http://localhost:11434`) with no `--specialized-agent` flag and no agents dir.
- `claude_args:` — extra args always forwarded to the in-jail `claude` (before any `-- <args>` from
  the command line).
- `mcp_log_level:` — `RUST_LOG` for the in-jail `tddy-tools --mcp` server (see "Observability"
  below).

`tddy-sandbox-app`'s `config::resolve_session_agents` merges named + inline + `agents_dir` defs;
`--model` is optional (defaults after config merge).

### Egress: plain-HTTP forward proxy

The sandbox egress shim was originally CONNECT/HTTPS-only, so the subagent's plain-HTTP
`POST http://localhost:11434/...` (absolute-form via `HTTP_PROXY`) hit the shim's "everything else
→ 404" branch. The shim now also supports a **forward-proxy path**:
`rewrite_http_proxy_request` rewrites absolute-form → origin-form and extracts host:port;
`handle_http_forward` opens a relay tunnel (the host owns the outbound socket — no jail net rule
needed) and streams. The CONNECT handler was refactored to share `open_relay_tunnel` +
`pump_tunnel`. As a result, `base_url: http://localhost:11434` works as-is — in managed mode the
subagent's HTTP to `localhost` is relayed to the host by the egress shim (the same mechanism the
default FastContext `:30000` endpoint already relies on).

### Context size for Ollama-hosted models

Ollama's `/v1/chat/completions` endpoint **cannot set `num_ctx` per request** (rejected upstream —
ollama/ollama#6137); its 4096 default is too small for repo exploration (the model overruns and
loops). The Ollama-recommended route is a Modelfile variant that bakes the context length into a
named model: `fastcontext-tools-32k.Modelfile` (`FROM fastcontext-tools:latest` +
`PARAMETER num_ctx 32768`) → `ollama create fastcontext-tools-32k`. Point the config's `model:` at
the variant. No code change — the sandbox config's existing `model:` field is the knob.

### Observability: persisted MCP/subagent logs

`write_claude_mcp_config` writes an `env` block for the `tddy-tools --mcp` server; the runner sets
`TDDY_TOOLS_LOG_FILE` → `<session-dir>/egress/tddy-tools.mcp.log` and `RUST_LOG` (default
`info,tddy_tools=debug,tddy_discovery=debug`, override via `mcp_log_level` config / `--mcp-log-level`
CLI / runner `--mcp-log-level`). `tddy-tools`' `init_logging()` honors `TDDY_TOOLS_LOG_FILE` (append;
falls back to stderr). The app also maintains a `<session-base>/sessions/latest` symlink to the
newest session dir.

Per-turn subagent logging (`tddy-discovery::subagent`, target `tddy_discovery::subagent`) logs each
turn: request (model, message/tool counts), completion (elapsed, `finish_reason`, content length,
tool-call count), and errors. Combined with the runner's `TDDY_TOOLS_LOG_FILE` wiring, fastcontext's
behavior lands in `<session>/egress/tddy-tools.mcp.log` instead of being invisible.

### Replaced-tool enforcement (defense-in-depth)

Dropping a replaced tool from `--allowedTools` only un-pre-approves it — Claude's native built-in
(`Grep`/`Glob`) and the still-advertised `mcp__tddy-tools__*` form remained reachable via the
permission prompt. Enforcement is now layered:

1. **`--disallowedTools`** (`tddy-sandbox-recipes/src/claude_cli.rs`): `append_claude_mcp_args` also
   emits `--disallowedTools <native>` + `--disallowedTools mcp__tddy-tools__<tool>` for each replaced
   tool, so they are unreachable. The builtin `fastcontext` def's `replaces` includes `SemanticSearch`
   (delegated to fastcontext / disabled for the main agent).
2. **Server-side enforcement** (`tddy-tools` `PermissionServer::new()` in `server.rs`): filters the
   advertised exec catalog by the replaced set
   (`resolve_replaced_tools_for_defs(&subagents_from_env())`) before merging it into the tool router,
   so a replaced tool is not advertised and cannot be invoked at the server — independent of Claude's
   allow/disallow lists. The subagent's own READ/GLOB/GREP loop is a separate in-process path
   (unaffected), so delegation still works.

### Runner `--claude-arg` pass-through

`tddy-sandbox-runner` gained a repeated `--claude-arg` (`allow_hyphen_values`), appended verbatim to
the in-jail `claude` argv **after the fixed flags and before the MCP args** (the MCP block's trailing
`--mcp-config` is variadic and would otherwise swallow a trailing positional prompt). Ignored in
`--pty-command` mode. `SpawnParams` now carries resolved `specialized_defs` + `claude_args` (replaces
the old `SubagentSpawnConfig`; resolution moved to `config.rs`).

### Deferred (Phase 2)

- **Integration/acceptance test** exercising a full sandboxed launch with an inline Ollama def — the
  launcher was verified by a manual full-launch smoke test (config loads → `codebase_mode=managed` →
  inline `fastcontext` activated, end-to-end through a real macOS Seatbelt jail), but the interactive
  terminal-attach path was not exercised in CI and no automated regression test exists. Tracked in
  `docs/dev/TODO.md`.
- A **dedicated feature doc file** for the launcher is not split out separately; this section is the
  home for that knowledge. If it outgrows this file it can be lifted into its own
  `docs/ft/coder/claude-sandbox-launcher.md` later.
