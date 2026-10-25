# 2026-10-24 — The main agent can hand a subagent's permission request to you

When a coding subagent asks permission and the main agent is unsure, the main agent can hand the
request to you. The subagent's bubble then reads **waiting for you**, and its conversation shows the
request with the agent's options as buttons; your choice goes to the subagent and the main agent
carries on with the rest of the turn. The entry records who answered and with what — or who cancelled
it. The main agent can still answer it itself until you do; whichever answer arrives first wins. It
works for a conversation another daemon runs, and only you, never the agent's own credential, can
answer as you. ([#727](https://github.com/uppin/tddy-coder/pull/727), `#coagents` 8/8)

Feature doc: [agent activity pane](../agent-activity-pane.md#the-user-answers-a-deferred-permission-request-added-2026-10-25).
