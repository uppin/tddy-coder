# 2026-10-28 — A claude-cli session says what it is asking

When a claude-cli session waits on a permission or asks a question, the daemon now knows **what** it
asks: a permission's tool and input, or every question with its options. The session's pending
question (`ReadSessionActivity`) carries it under a stable `request_id`, `pending_elicitation` is set
while it waits, and Telegram says "*Session X wants to run* `Bash`: `cargo publish`" or lists the
question and its options, once per request, instead of "needs your input" — split into several
messages when a question set is too long for one. Claude's own prompt is unchanged and still the only
place to answer ([#768](https://github.com/uppin/tddy-coder/pull/768)). See
[Claude Code CLI session § What a session is asking](../claude-cli-session.md#what-a-session-is-asking)
and [Telegram § What the session is asking](../telegram-notifications.md#what-the-session-is-asking).
