# 2026-08-30 — Subagents as children of the main agent in the Agents tab

- **The Agents tab is now a tree.** The session's own main agent sits at the root, with everything working for it beneath: the specialized agents attached to it *and* the sessions it spawned.
- **A spawned subagent shows what its agent is doing**, not what its session is. It used to be listed in the detail pane labelled "active" — the session's lifecycle, which says nothing about the agent inside it. It now carries the same badge a roster agent does: running, executing tool, waiting for input.
- **A subagent that spawned its own subagent nests under it**, so a chain of delegation reads as a chain instead of a row of siblings. Its own attached agents nest there too, once you open it.
- **Opening a subagent is what reads its roster** — a closed one costs nothing, and still says what it is doing.
- **A subagent row offers "Switch"** to focus that session; attached agents keep "Detach", which still names the host whose checkout a detach would delete.
- **"Unknown" is never shown as "idle".** A subagent the daemon does not watch says so honestly rather than being reported free for work.
- **A subagent whose agent list could not be read says why**, on its own row — an unreadable list is not an empty one.
- **The "Session agents" section is gone from the session detail pane.** Its peers are a branch of the tree now, and the Agents tab is the one place they are listed.
