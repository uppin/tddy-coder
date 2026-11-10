# `catalog.CatalogService`

Family **A** — the tools-and-agents catalogue the daemon exposes to pickers and attach flows.

| RPC | Purpose |
|-----|---------|
| `ListTools` | Allowed `tddy-*` binaries from config (`allowed_tools`). |
| `ListAgents` | Allowed coding backends from config (`allowed_agents`), then registry assistants, then [harness profiles](../../../docs/ft/daemon/harness-profiles.md) from `<tddy_data_dir>/agents` (`harness_profile::harness_profile_rows`: id = profile name, label = `label` or name; appended in `tddy-daemon-rpc`'s `catalog/ports.rs` `list_agents`). Each row is annotated from the daemon's coding-agent registry (`catalog/agent_rows.rs`): an id naming a spec is `kind = CODING_AGENT` with `harness` (the spec id) and `route_providers` (the spec's `route.providers`, empty for an agent that always runs on its own vendor); any other id is `kind = MODEL`. |
| `ListAgentModels` | Models available from the daemon's registry integration. |
| `ListSubagents` | Specialized agent defs this daemon can attach — `<tddyhome>/agents/*.yaml` and registry assistants, answered from the same resolution path as attach. See [specialized-subagents.md](../../../docs/ft/coder/specialized-subagents.md). |

## Where it is served

- **`tddy-discovery`** implements the handlers (`catalog_service.rs`, `agent_list_mapping.rs`).
- **`tddy-daemon`** registers `build_catalog_entry` on Connect-HTTP, the LiveKit common room, session
  rooms, and the local Unix socket beside the other unbundled families.

The crate takes **allowlist rows**, not `DaemonConfig`: `main.rs` extracts
`agent_list_mapping::agent_allowlist_rows(&config, &[])` and passes rows in.

## Clients

- **`tddy-web`** — `useAvailableAgents`, `useSelectableAgents`, `useAgentModels`, create-session pickers.
- **Peer attach** — a facilitating daemon forwards `ListSubagents` to an owning peer at
  `catalog.CatalogService` when resolving a qualified `name@daemon_instance_id`.

Remote tool dispatch in `tools.rs` uses a generated Connect client to
`exec_tools.ExecToolService/ExecuteTool`, not a hand-built URL.
