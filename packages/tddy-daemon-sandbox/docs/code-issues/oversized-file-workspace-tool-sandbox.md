# oversized-file: workspace_tool_sandbox.rs

**Location:** `packages/tddy-daemon-sandbox/src/workspace_tool_sandbox.rs`
**Category:** oversized-file
**Detected:** 2026-09-19 — `/pr-wrap` step 3.5 file-length gate
**Metrics:** **614 production lines** · budget 500
**Thresholds breached:** length 614 > 500
**Restructure:** `extract_module --to_file` — single seam, designed, **passes a plain `check`**, not applied
**Status:** Open — **unclaimed**

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-19 | 590 | 522 → 571 for `stop_all()`, then → 590 for the registry `Drop` |
| 2026-09-26 | 614 | `ToolDispatchOutcome` added to the trait and its impl, and the trait doc rewritten to say why the distinction exists |

## What would close it — designed seam

**Seam:** contiguous run L245–507, the whole jail implementation — `JAIL_READY_TIMEOUT`,
`JailedWorkspaceSandboxProvisioner`, `impl WorkspaceSandboxProvisioner for …`, `start_jail_channel`,
`prepare_jail_tree`, `canonical`, `canonical_exec`, `InJailChannel`, `JailedWorkspaceSandbox`,
`impl WorkspaceSandbox for …`, `impl Drop for …`, `exchange_in_jail_tool_call`.

**Operation:** `extract_module --to_file`, name `jailed_workspace_sandbox`, `reexport: "named"`.

**Estimate:** ~263 lines out → parent **~308**. A single seam clears the budget.

**Verified:** this plan passes a plain (non-deep) `check` — `no findings`. It has **not** been proven
by `check --deep`, which is the real gate.

## Constraints a later session must know

Line numbers above were taken at 571 production lines and have shifted by the registry `Drop` added
at ~`:549`. Re-derive them with `restructure anchors`; never carry hand-written coordinates across a
file edit.
