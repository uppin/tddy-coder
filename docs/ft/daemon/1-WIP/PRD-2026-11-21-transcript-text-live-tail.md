# PRD — A claude-cli session's prompt and reply reach a live transcript tail

A client following a claude-cli session's transcript live (`StreamAcpReplay`) receives the user's
prompt and the agent's reply as they are reported, in order among the tool calls, at the position a
later re-read would give them.

- A `UserPromptSubmit` report delivers a user-message frame; a `Stop` report an agent-message frame.
- Count-first subscribers and `StreamSessionActivity` are unaffected.
- The wire shape of `StreamAcpReplay` does not change.
