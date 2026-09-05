# 2026-08-19 — Session agent catalog across hosts

- **The New-session Agent dropdown now lists the agents of every connected host**, each labelled with the host that offers it — an assistant created on one machine is selectable from a browser pointed at another, which it previously was not. See [session-agent-catalog-fan-out.md](../session-agent-catalog-fan-out.md).
- **Picking an agent sets the session's Host**, and changing the Host re-points the agent to that host's agent of the same name (else its first) — the pair on screen can no longer name a host that does not offer the agent.
- **Two hosts offering an agent of the same name are two separate choices**, distinguishable in the list rather than collapsed into one ambiguous row.
- **A host that cannot be reached costs one error row, not the dropdown**: it is named, with the daemon's own words, while every other host's agents stay selectable. Nothing is silently omitted.
- **Nothing offered shows "No agents available"** instead of an empty control.
- **Adding an agent to an existing session offers only the agents of the host that session runs on**, since a peer joins its worktree and cannot run elsewhere.
- Unchanged, and worth knowing: the **model** list still comes from the app-level selected host, so where two hosts run different backends a peer host's agent lists the wrong host's models. See [tool-session-model-selection.md](../tool-session-model-selection.md).
