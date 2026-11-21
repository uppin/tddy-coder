# 2026-11-21 — A claude-cli session's transcript holds what was said

Each prompt a claude-cli session is given is a user message and each reply it finishes a turn with an
agent message in its transcript, in order among the tool calls, for local and target sessions alike.
A turn with no reply text adds no message. A live tail follows tool calls only for now. See
[Claude CLI session § The transcript holds what was said](../claude-cli-session.md#the-transcript-holds-what-was-said).
