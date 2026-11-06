# 2026-11-06 — The bundle hard-codes no agent; metadata comes from the daemon

**Type:** Refactor

`#agent-tools` 20/28 ([#828](https://github.com/uppin/tddy-coder/pull/828)).

- `SESSION_TYPE_OPTIONS`, the per-agent session-type checks and the per-agent Create-session
  field components are deleted. Session types, labels, capabilities, argv params (model /
  permission mode / bypass / prompt), permission modes, default model, placements and route
  providers are read from `CatalogService.ListAgents` rows (`AgentInfo` fields 6–11) for the
  target daemon, and from the serving daemon's injected `GET /api/config`
  (`ClientConfig.allowed_agents`) for its own selection.
- The Create-session pane derives everything from the served agent: the model field mounts
  when its argv binds `model`, the Sandbox toggle when its placements include `jail`, the
  Sandboxed-codebase toggle when it declares `split_host`, the provider field when it declares
  route providers. The sessions drawer registers the served session types from its own
  `ListAgents` listing, so terminal-tab kinds no longer depend on a Create pane having mounted.
- Cypress fixtures serve the wire a daemon serves (`cypress/support/servedAgents.ts`, merged by
  the shared response builders); the bundle still names no agent. See the
  [cross-package entry](../../../../docs/dev/changesets/2026-11-06-agent-tools-sweep-yaml-only.md).
