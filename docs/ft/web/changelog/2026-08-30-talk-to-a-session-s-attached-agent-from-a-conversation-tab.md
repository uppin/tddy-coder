# 2026-08-30 — Talk to a session's attached agent from a conversation tab

- **"Add agent" in the session header attaches a specialized agent to the session you are looking at**, instead of spawning a second coding session on its worktree. The old flow cost a whole session to do what the agent roster already does properly.
- **The attached agent gets a conversation tab** beside the Agent and bash tabs: type a prompt, watch the answer stream in. Closing the tab ends the conversation; nothing else does — selecting another session leaves it open.
- **The picker names what the main agent loses before you confirm** — every tool the agent replaces stops being callable by the main agent while it stays attached — and offers every common-room host's agents under their qualified `name@host` ids, since two hosts routinely offer an agent of the same name.
- **Attaching an agent that already has a tab focuses that tab** rather than opening a second, because the second attach is a no-op on the roster.
- **An agent that answers with nothing still shows a completed turn**, so "said nothing" never looks like "nothing arrived"; a refused open, a refused attach and a failed prompt are each named where they happened.
- **This is your conversation with the agent, not a replay of the main agent's.** A roster agent writes no transcript anywhere, so what the main agent asked it stays visible only as the roster row's status and last-activity line.
- The `#/sessions/:id/add-agent` route is gone, along with the create-session pane's peer mode. Peer *sessions* still exist and are still listed — they now arrive only from `spawn_conversation` and the PR-stack flow.
