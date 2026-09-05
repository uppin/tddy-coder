# 2026-03-29 — Connection screen: backend options from `ListAgents`

- **ConnectionScreen**: **Backend** (per project) is populated from **`ListAgents`**; option values are agent **`id`** strings from daemon **`allowed_agents`**, with labels from the RPC (blank optional labels resolve to **`id`** on the server). **Start New Session** requires a selected backend when the list is non-empty. The default selection is the first RPC entry unless a stored choice for that project still exists in the list.
- **RPC load**: **`ListTools`** and **`ListAgents`** run together after auth; either failure clears both dropdowns and shows the shared connection error message.
- **Tests**: Bun helpers in **`agentOptions.ts`**; Cypress component coverage for dynamic backend options and **`ListAgents`** intercepts.
- **Feature docs**: [web-terminal.md](../web-terminal.md) (Connection screen), [local-web-dev.md](../local-web-dev.md) (dev YAML tools + agents).
