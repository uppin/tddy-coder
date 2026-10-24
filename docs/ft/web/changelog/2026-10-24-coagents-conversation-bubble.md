# 2026-10-24 — A subagent conversation is one activity bubble that opens it

In a session's activity pane, the main agent's `subagent_*` calls for one conversation are now **one
bubble** showing the agent, what it was asked and a live status (running, waiting for permission,
ended, failed). Clicking it opens the conversation itself: the subagent's own transcript — its text,
its tool calls with their detail, its permission requests — live while it runs. A conversation that
ran in its jail says it was not recorded. ([#723](https://github.com/uppin/tddy-coder/pull/723),
`#coagents` 4/8)

Feature doc: [agent activity pane](../agent-activity-pane.md#a-subagent-conversation-is-one-bubble-added-2026-10-24).
