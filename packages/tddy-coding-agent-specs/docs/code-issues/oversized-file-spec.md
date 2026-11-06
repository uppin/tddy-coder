# oversized-file: spec.rs

**Location:** `packages/tddy-coding-agent-specs/src/spec.rs`
**Category:** oversized-file
**Detected:** 2026-11-04 by `/pr-wrap` step 3.5 file-length gate on PR #800 (`#agent-tools` 11/12)
**Metrics:** **711 production lines** (counted to the first `#[cfg(test)]`) · budget 500 · **1.42× over**
**Thresholds breached:** length 711 > 500
**Status:** Open — regressed 2026-11-04 (656 → 711, #801), unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-04 | 492 → 656 | first detection — PR #800 crossed it: `AcpSection` with `run_section()`, `CodingAgentSpec::{acp, acp_mcp, acp_tool_deny}`, the `ToolSlot::{AcpMeta, ConfigEnv}` variants and the `hooks_adapter` check in `CodingAgentExtension::validate`. Not split: the developer deferred all code restructuring of the `#agent-tools` stack to its sweep node (2026-11-04) |
| 2026-11-04 | 656 → 711 | **regressed** by PR #801 (`#agent-tools` 12/19), measured against `origin/feature/agent-tools/acp-agents`: `CodingAgentSection.{capabilities, answer_wait_secs}`, `DEFAULT_ANSWER_WAIT_SECS`, `CodingAgentSpec::{answer_wait_secs, capabilities, has, route_support}` and `RouteSlot.{providers, shape}`. Not split: restructuring deferred to the `#agent-tools` sweep node by the developer (2026-11-04). A `spec/capabilities.rs` sibling for the capability and route views would take ~50 lines out |
| 2026-11-05 | 711 → 736 | **regressed** by PR #819 (`#agent-tools` 13/19), measured at its green: `CodingAgentSpec::product`, `routed_agent` and the `product` field (the route types left for `tddy-agent-backend`, which took ~20 lines out). Not split: restructuring deferred to the `#agent-tools` sweep node (2026-11-04). The `spec/capabilities.rs` sibling named below now also holds `route_support`, `product` and `routed_agent` |

## What would close it

The ACP section is one group: `AcpSection`, its `run_section()` and the three `CodingAgentSpec::acp*`
accessors. Moving it into a `spec/acp.rs` sibling (engine-driven, `code-restructuring`), beside the
headless section if that is moved the same way, takes the file under budget.
