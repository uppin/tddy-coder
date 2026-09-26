# 2026-09-26 — A dead jail is rebuilt once, and a conversation's turn control crosses the wire

**A tool call sent into a workspace jail comes back as one of two outcomes**, not one: the tool
*ran* inside the jail (whatever it answered, `is_error` included), or the call **never reached a
tool**. The two were reported identically, deliberately, so a caller could not tell "the command
said no" from "the channel is dead" — and a dead channel stayed dead for the life of the session.

On the second outcome and only on it, the jail is torn down, re-provisioned and the call retried
**exactly once**. A non-zero exit never triggers a rebuild. A second transport failure in a freshly
spawned runner is reported rather than retried again. Neither the rebuild nor its failure is a
route onto the host worktree: a jail that cannot be rebuilt, and a replacement that dies the same
way, answer with the failure. The retry lives in `LocalExecTools`, the only layer holding the
sandbox registry *and* sitting beneath all three dispatch entries — the unary RPC, the streaming
RPC, and a roster agent's own turn loop.

Accepted cost: a transport failure says the *answer* did not come back, not that the tool did not
run, so a mutating call retried in a rebuilt jail can execute twice
(`docs/dev/todo/2026-09-26-a-jail-rebuild-can-re-run-a-tool-call-that-already-executed.md`). Jail
death is still discovered per call rather than watched for, and the seam is proven with a test
double because the real-jail suite runs on no CI machine — §2 of
`docs/dev/todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md` is partly closed,
its shutdown half untouched.

**`Read` honours `offset` and `limit`.** The catalog has advertised both since it was written and
the engine implemented neither, so a jailed subagent's 200-line cap bounded nothing. `Read` now
returns `{content, truncated, total_lines}`; a window past end-of-file is empty rather than an
error, and a bare `Read` still returns the whole file byte for byte.

**The conversation RPCs carry turn control.** `session_agents.SessionAgentService` gains
`ResumeAgentConversation` (server stream), `max_turns` on prompt and resume, `from_message_id` and
`correction` on resume, and `messages` + `clamped_max_turns` on the final `AgentConversationChunk`.
A jail holds no agent definition, so every subagent conversation in a jailed deployment is served
here: until these crossed the wire the remote half returned no history and could not be rewound
into at all. Ten methods now, six of them in the in-jail relay allowlist.

`ResumeAgentConversation` earns its allowlist entry because a conversation an in-jail `tddy-tools`
opened over the relay must be one it can continue. It reaches the same code path as
`PromptAgentConversation` under the same authentication, so it opens no route weaker than one
already open — and the justification stops there: a caller is **not** confined to its own session's
conversations, a pre-existing gap recorded in
`docs/dev/todo/2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it.md`. What
resume adds over prompt is a destructive write, not one more turn.

**Not proven across the wire.** `ResumeAgentConversation` is compile-checked and covered either
side of the coordinate, but no test drives the RPC end to end — the suite that would runs on no CI
machine (`docs/dev/todo/2026-09-26-the-resume-rpc-and-its-turn-budget-have-no-wire-level-test.md`).

[remote-codebase-mode.md](../remote-codebase-mode.md) §§ Remote daemon: tool execution, What else a
jailed agent may reach, Workspace tool sandbox ·
[session-agent-roster.md](../session-agent-roster.md) § A conversation's turn control crosses the
wire ·
[docs/dev/changesets/2026-09-26-subagent-turn-control-and-honest-tool-failure.md](../../../dev/changesets/2026-09-26-subagent-turn-control-and-honest-tool-failure.md)
