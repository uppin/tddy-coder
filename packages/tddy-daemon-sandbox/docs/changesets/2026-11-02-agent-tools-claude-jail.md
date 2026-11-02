# 2026-11-02 — tool_jail: the jail of a tool run from its definition

**Type:** Feature

`tool_jail::{lower_tool_jail, ToolJailInputs, ToolJailGrants, build_tool_jail_plan, route_hosts}` — a provider route's base-URL host joins the egress policy. `sandbox_session::dial_and_bridge_with_egress` holds the host relay to the jail's `EgressGuard`; `prepare_persistent_claude_home` has no daemon caller (kept for #799). Cross-package: [agent-tools-claude-jail](../../../../docs/dev/changesets/2026-11-02-agent-tools-claude-jail.md).
