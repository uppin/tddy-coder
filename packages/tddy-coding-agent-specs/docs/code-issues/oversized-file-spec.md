# oversized-file: spec.rs

**Location:** `packages/tddy-coding-agent-specs/src/spec.rs`
**Category:** oversized-file
**Detected:** 2026-11-04 by `/pr-wrap` step 3.5 file-length gate on PR #800 (`#agent-tools` 11/12)
**Metrics:** **981 production lines** (2026-11-06, counted to the first `#[cfg(test)]`) · budget 500 · **1.96× over**
**Thresholds breached:** length 981 > 500
**Status:** Open — regressed 2026-11-06 (815 → 981, #828), unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-04 | 492 → 656 | first detection — PR #800 crossed it: `AcpSection` with `run_section()`, `CodingAgentSpec::{acp, acp_mcp, acp_tool_deny}`, the `ToolSlot::{AcpMeta, ConfigEnv}` variants and the `hooks_adapter` check in `CodingAgentExtension::validate`. Not split: the developer deferred all code restructuring of the `#agent-tools` stack to its sweep node (2026-11-04) |
| 2026-11-04 | 656 → 711 | **regressed** by PR #801 (`#agent-tools` 12/19), measured against `origin/feature/agent-tools/acp-agents`: `CodingAgentSection.{capabilities, answer_wait_secs}`, `DEFAULT_ANSWER_WAIT_SECS`, `CodingAgentSpec::{answer_wait_secs, capabilities, has, route_support}` and `RouteSlot.{providers, shape}`. Not split: restructuring deferred to the `#agent-tools` sweep node by the developer (2026-11-04). A `spec/capabilities.rs` sibling for the capability and route views would take ~50 lines out |
| 2026-11-05 | 711 → 736 | **regressed** by PR #819 (`#agent-tools` 13/19), measured at its green: `CodingAgentSpec::product`, `routed_agent` and the `product` field (the route types left for `tddy-agent-backend`, which took ~20 lines out). Not split: restructuring deferred to the `#agent-tools` sweep node (2026-11-04). The `spec/capabilities.rs` sibling named below now also holds `route_support`, `product` and `routed_agent` |
| 2026-11-05 | 736 → 769 | **regressed** by #820 (`#agent-tools` 14/19): `CodingAgentSection.conversation`, `CodingAgentSpec::conversation()` (the derived default) and the two conversation load refusals in `CodingAgentExtension::validate`; measured on `origin/feature/agent-tools/route-support` (`3c29698cd`) and HEAD at #820's wrap. Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer (2026-11-04). The conversation accessor and its refusals would move with the `spec/capabilities.rs` sibling named below |
| 2026-11-05 | 769 → 815 | **regressed** by #821 (`#agent-tools` 15/19): `HeadlessSection`'s seven protocol fields (`system_prompt`, `goal_params`, `approval_tool`, `workflow_mcp`, `route_model`, `fresh_session`, `scratch`) and `HeadlessSection::protocol()`, plus their load refusals; measured against `origin/feature/agent-tools/cli-spec-features` (`4552a851c`) at #821's wrap. Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer (2026-11-04). The headless section would move to a `spec/headless.rs` sibling the same way as the ACP one below |
| 2026-11-06 | 815 → 981 | **regressed** by the sweep (#828, `#agent-tools` 20/28): the CLI-arg schema the deleted descriptor fields needed — `argv_params`, `binds_argv_param`, `render_run_argv`, `model_listing_argv`, `curated_models`, `CuratedModel`, `permission_modes`, `transcript_usage`. The sweep deleted metadata consumers, not this file's seams, and did not split it: the restructuring the 2026-11-04 deferral pointed at this node remains to be scheduled. Counted to the first `#[cfg(test)]` on the sweep branch |

## What would close it

The ACP section is one group: `AcpSection`, its `run_section()` and the three `CodingAgentSpec::acp*`
accessors. Moving it into a `spec/acp.rs` sibling (engine-driven, `code-restructuring`), beside the
headless section if that is moved the same way, takes the file under budget.
