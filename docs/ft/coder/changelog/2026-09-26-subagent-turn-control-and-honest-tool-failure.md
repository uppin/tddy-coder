# 2026-09-26 — Subagent turn control, and a subagent that says when it read nothing

A specialized subagent could spend its whole turn budget on tool calls that were every one refused
and still answer with a confident, file:line-cited summary of nothing. On 2026-09-26 one did: 54
refused calls, then an invented line number, constant value and working directory — twice, byte for
byte, because turns go out at `temperature: 0.0`.

**A turn in which no tool call succeeded is now an error**, returned before any model call, naming
the transport failure. No synthesis turn, so there is nothing to invent from. The conversation is
left exactly as the failed call built it and stays promptable and rewindable. A tool that ran and
exited non-zero is not this condition, and a call in which some tools worked keeps the
`max_turn_requests` soft landing. The guard sits on the budget-exhaustion path alone — a model that
ends the turn early after every tool call failed still returns its answer
(`docs/dev/todo/2026-09-26-the-honest-failure-guard-does-not-cover-a-model-ended-turn.md`).

**`maxTurns`** on `subagent_prompt` and `subagent_resume`: how many model turns *this one call* may
spend, in place of the agent definition's budget. Bounded to `1..=50`, clamped to the nearer bound
rather than refused, and the clamp reported as `clampedMaxTurns` so a caller never reads an early
stop as a finished search. A definition's own budget is the operator's and is never clamped. The
floor is as load-bearing as the ceiling: a zero budget would run no turn, so no tool call could
fail, so the outage guard would have nothing to fire on and control would reach the synthesis turn
with a history holding only the prompt.

**Every turn outcome enumerates the messages it appended** — `{id, role, tool, tool_calls,
is_error, preview}` per message, previews cut to 240 characters. `is_error` is the field whose
absence let the incident run: a main agent could see how many messages went by, not that every one
was a refusal.

**`subagent_resume`** takes another turn without asking anything new. `fromMessageId` sends the
conversation back to a message it holds, discarding what follows and keeping a tool call with its
results; `correction` appends exactly one corrective instruction after that point, which is what
makes a rewind able to change anything at `temperature: 0.0`. Ids are never reused, so an id held
from before a rewind addresses nothing rather than a different message. Unknown ids are errors, never
a silent continue. It queues, reports `queuePosition`/`queueSize`, and defers with a `responseId`
exactly as a prompt does.

**Two defects on the same path, fixed rather than stepped around.** No shell the tool engine starts
inherits the runner's IPC standard input, and one that overruns its budget has its whole process
group signalled — the wedge that started the incident was `grep` with an empty file argument
becoming a rival reader on the daemon→jail request pipe, then outliving the timeout that dropped
it. And `Read` honours the `offset`/`limit` it has advertised since the catalog was written, so a
jailed subagent's 200-line cap is applied on the Managed path as well as the Local one.

`subagent_resume` is advertised in the MCP catalog and present in the sandboxed-Claude allowlist;
the advertisement audit moves 43/40 → 44/41.

**Not proven end to end.** Every conversation in a jailed deployment is remote, so all of this
crosses `session_agents.SessionAgentService` — and no test drives `ResumeAgentConversation` across
that wire, because the suite that would runs on no CI machine
(`docs/dev/todo/2026-09-26-the-resume-rpc-and-its-turn-budget-have-no-wire-level-test.md`).

[managed-codebase-subagents.md](../managed-codebase-subagents.md) §§ ACP → MCP tool mapping, Turn
control · [sandboxed-codebase-mode.md](../sandboxed-codebase-mode.md) § Nothing the jail runs can
read the channel it is served over ·
[remote-codebase-mode.md](../../daemon/remote-codebase-mode.md) §§ Remote daemon: tool execution,
Workspace tool sandbox · [session-agent-roster.md](../../daemon/session-agent-roster.md) § A
conversation's turn control crosses the wire ·
[docs/dev/changesets/2026-09-26-subagent-turn-control-and-honest-tool-failure.md](../../../dev/changesets/2026-09-26-subagent-turn-control-and-honest-tool-failure.md)
