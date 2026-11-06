# oversized-file: spec.rs

**Location:** `packages/tddy-coding-agent-specs/src/spec.rs`
**Category:** oversized-file
**Detected:** 2026-11-04 by `/pr-wrap` step 3.5 file-length gate on PR #800 (`#agent-tools` 11/12)
**Metrics:** **656 production lines** (counted to the first `#[cfg(test)]`) · budget 500 · **1.31× over**
**Thresholds breached:** length 656 > 500
**Status:** Open — unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-04 | 492 → 656 | first detection — PR #800 crossed it: `AcpSection` with `run_section()`, `CodingAgentSpec::{acp, acp_mcp, acp_tool_deny}`, the `ToolSlot::{AcpMeta, ConfigEnv}` variants and the `hooks_adapter` check in `CodingAgentExtension::validate`. Not split: the developer deferred all code restructuring of the `#agent-tools` stack to its sweep node (2026-11-04) |

## What would close it

The ACP section is one group: `AcpSection`, its `run_section()` and the three `CodingAgentSpec::acp*`
accessors. Moving it into a `spec/acp.rs` sibling (engine-driven, `code-restructuring`), beside the
headless section if that is moved the same way, takes the file under budget.
