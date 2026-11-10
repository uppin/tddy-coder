# 2026-11-10 — spec.rs split into spec/ submodules

**Type:** Refactor

`#agent-tools` 33/33 ([#876](https://github.com/uppin/tddy-coder/pull/876)), plans 01–06 in
`docs/dev/1-WIP/2026-11-10-agent-tools-sweep-plans/`, moved through `tddy-tools restructure` and
verified statement-by-statement.

- `spec.rs` 1111 → 378 production lines. New submodules: `spec/headless_section.rs` (HeadlessSection,
  its validation and the impl members reading it), `spec/acp.rs` (AcpSection, AcpCommandError,
  package_runner), `spec/slot_shapes.rs` (slot shapes), `spec/route_slot.rs` (RouteSlot, ResumeSlot,
  Placement, PlacementError, routed_agent, route_slot_errors), `spec/pass_env.rs`
  (CodingAgentSpec::env_passes and its helpers), `spec/argv.rs` (argv/rendering members,
  CuratedModel, ModelsSection).
- All paths re-exported, so no caller moved; the latency acceptance tests cover the split.
- Tier C: slot rendering extracted from `headless.rs`'s `render` (72 → under 60 lines).
- Code issue `oversized-file-spec` closed at 378 and deleted.
