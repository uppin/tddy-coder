# 2026-09-26 — Ten methods: the conversation service serves resume, a budget and a transcript

**Type:** Feature

`ResumeAgentConversation` is served here and peer-routed like the other conversation RPCs, taking
the family from nine methods to ten and the in-jail relay allowlist from five to six. A turn honours
a per-call `max_turns`, applies the rewind and correction a resume carries, and emits
`AgentMessageDescriptor`s plus `clamped_max_turns` on the final chunk.

This matters more than a Rust-level change would: a jail holds no agent definition, so **every**
subagent conversation in a jailed deployment is served here rather than run in the caller's process.
Until these fields crossed the wire the remote half returned no history at all and could not be
rewound into.

**Not proven across the wire.** `ResumeAgentConversation` is compile-checked, clippy-clean and
covered either side of the coordinate, but no test drives the RPC end to end — the suite that would
is `in_jail_conversation_acceptance.rs`, which runs on no CI machine. Recorded in
[`docs/dev/todo/2026-09-26-the-resume-rpc-and-its-turn-budget-have-no-wire-level-test.md`](../../../../docs/dev/todo/2026-09-26-the-resume-rpc-and-its-turn-budget-have-no-wire-level-test.md).

[session-agent-service.md](../session-agent-service.md) · [../../../../docs/dev/changesets/2026-09-26-subagent-turn-control-and-honest-tool-failure.md](../../../../docs/dev/changesets/2026-09-26-subagent-turn-control-and-honest-tool-failure.md)
