# 2026-10-04 — `restructure_tools`: the six restructure tool definitions, advertised on the host's gate

**Type:** Feature

`#live-plan` 14/15, PR [#573](https://github.com/uppin/tddy-coder/pull/573). Cross-package entry:
[2026-10-04-session-restructure-tools.md](../../../../docs/dev/changesets/2026-10-04-session-restructure-tools.md).

`src/restructure_tools.rs` defines the six tools with their JSON schemas and a router; `PermissionServer::new`
merges them only when `TDDY_RESTRUCTURE_TOOLS` is set. `TDDY_RESTRUCTURE_TOOLS` joins the advertisement
audit's environment keys; the existing audits are unchanged with the gate unset, and
`restructure_tools_are_advertised_only_when_the_host_sets_the_gate` pins the gated set.
