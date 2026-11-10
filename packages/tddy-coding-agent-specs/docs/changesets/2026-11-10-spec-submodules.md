# 2026-11-10 — spec.rs split into spec/ submodules

**Type:** Refactor

`#agent-tools` 33/33 ([#876](https://github.com/uppin/tddy-coder/pull/876)). Every move went through
`tddy-tools restructure` (`move_item` / `move_impl_members`, one plan per seam group) and was verified
statement by statement with `restructure verify`.

- `spec.rs` keeps the spec struct, its parsing and validation; the sections live in
  `spec/headless_section.rs` (`HeadlessSection`, `HeadlessPermissionPolicy`, `CodingAgentSpec::headless`),
  `spec/acp.rs` (`AcpSection`, `AcpCommandError`, `package_runner`, `CodingAgentSpec::{acp, acp_command,
  acp_mcp, acp_tool_deny}`), `spec/slot_shapes.rs` (`McpSlot`, `McpFormat`, `ToolSlot`, `FlagSlot`,
  `InstructionsSlot`, `HooksSlot`), `spec/route_slot.rs` (`RouteSlot`, `ResumeSlot`, `Placement`,
  `PlacementError`, `route_slot_errors`, `routed_agent`), `spec/pass_env.rs`
  (`CodingAgentSpec::{pass_env, passed_env}` and the env-entry checks) and `spec/argv.rs`
  (`CodingAgentSpec::{argv_params, render_run_argv, model_listing_argv, curated_models, …}`,
  `CuratedModel`, `ModelsSection`).
- Every public path is re-exported, so no consumer changed.
- `headless.rs` `render`: 72 → 49 lines (slot rendering extracted).
- Code issue `oversized-file-spec` **closed and deleted**: 1,111 production lines by its record's
  measurement (1,152 by the `/pr-wrap` gate's count at the merge-base) → **378**.
