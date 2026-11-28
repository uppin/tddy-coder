# 2026-11-28 — An expired session token is refused as expired

**Type:** Feature · **PR:** [#1064](https://github.com/uppin/tddy-coder/pull/1064)

`authorize_exec_tool_caller` reads the unverified `exp` of a token its user resolver rejects: a
token whose expiry is in the past is refused as expired — how long ago, and the way out (the
daemon resumes the session, which re-mints the agent's token) — instead of the split-session
key-advertisement hint, which blamed a transient key gap for what a plain expiry caused. An
unparseable token keeps the verification hint. The claim is read without verifying the token: it
picks the message, never whether the call is allowed. See
[session-agent-roster.md](../session-agent-roster.md).
