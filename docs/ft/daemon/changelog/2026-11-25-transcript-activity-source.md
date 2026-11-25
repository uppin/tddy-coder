# 2026-11-25 — A Cursor session's feed carries its whole conversation, and target hooks are no longer lost

- A Cursor session's activity (`ReadSessionTranscript`, `ReadSessionActivity`, the web Agent Activity panel) now holds the user's prompts and every reply Cursor writes — the text between its tool calls included — among its tool calls, in Cursor's order. The words are read from Cursor's own transcript file, followed live on this host and on a Coder or SSH target (agent in tmux or not); the tool calls still come from its hooks. Where an agent's activity comes from is declared in its spec (`coding_agent.activity`).
- A target session in tmux keeps reporting when the daemon's connection to it drops or the daemon restarts: the hook log is no longer removed while the agent still runs, a resume keeps it, and its follower resumes from the last line it delivered — over a Coder pty too.
- Cursor's tool-call ids no longer carry a newline.

Feature: [cursor-cli-session.md § Activity from the transcript](../cursor-cli-session.md#activity-from-the-transcript) · [claude-cli-session.md § Following a target's hook log](../claude-cli-session.md#following-a-targets-hook-log)
