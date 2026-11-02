# oversized-file: workspace_tool_sandbox.rs

**Location:** `packages/tddy-daemon-sandbox/src/workspace_tool_sandbox.rs`
**Category:** oversized-file
**Detected:** 2026-09-19 — `/pr-wrap` step 3.5 file-length gate
**Metrics:** **684 production lines** (2026-11-02) · budget 500
**Thresholds breached:** length 684 > 500
**Restructure:** `extract_module --to_file` — single seam, designed, **passes a plain `check`**, not applied
**Status:** Open — partially fixed (the ripgrep patch is gone; the file is still 184 over budget) — **unclaimed**

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-19 | 590 | 522 → 571 for `stop_all()`, then → 590 for the registry `Drop` |
| 2026-09-26 | 614 | `ToolDispatchOutcome` added to the trait and its impl, and the trait doc rewritten to say why the distinction exists |
| 2026-09-26 | 660 | +46 for `host_ripgrep_dir` and the two grants that make `Grep` reachable inside a jail. Marked `FIXME(grep-in-jail)`; the real fix removes it again — see `docs/dev/todo/2026-09-26-grep-is-unreachable-inside-every-jail.md` |
| 2026-09-27 | 737 | +73 fixing the ripgrep grants: `which` reports a symlink into `Cellar`, `rg` links a Homebrew `libpcre2`, and the loader traverses a symlink farm the renderer canonicalizes away. Still `FIXME(grep-in-jail)`; PR #548 deletes all of it |
| 2026-10-01 | 739 | **worse** by 2 in PR #560 (`#agent-worktree` 1/4): the `conversation_id` parameter and its doc on the jail route's `execute_tool` request. Inside the standing deferral (PR #548 deletes the ripgrep block this file is over budget for) |
| 2026-10-22 | 741 | **worse** by 2 in PR #670 (`#provider-route` 9/9): `route: None` and its comment on the workspace tool jail's plan — the jail hosts no agent, so it takes no route. Inside the standing deferral |
| 2026-11-02 | 684 | **better** by 57 in PR #790 (`#agent-tools` 1/12): `host_ripgrep_grants`, `RipgrepGrants`, `binary_dependency_traversal_dirs` and their three `FIXME(grep-in-jail)` sites are deleted; the jail now lowers declared tools through `tddy_sandbox_recipes::tool_grants`. The provisioner gained tool resolution at jail start (`new`, `for_tddy_home`, `resolve_jail_tools`, `WORKSPACE_JAIL_TOOLS`). The standing deferral's premise (PR #548 deletes the ripgrep block) is void: the block is gone and the file is still over budget, so the seam below is what closes it |
| 2026-11-02 | 713 | **worse** by 29 in PR #792 (`#agent-tools` 3/12): `build_workspace_tool_plan` lowers its tools' network (`lower_network`, the shim port, `--egress-*` runner args, `loopback_allow_ports`, `NetworkSpec.egress` / `allow_oauth_inbound`). Not split: stack-shared with every `#agent-tools` node, deferred to after the stack lands |

## What would close it — designed seam

**Seam:** contiguous run from `JAIL_READY_TIMEOUT` (~L293 at 684 lines) to the end of
`exchange_in_jail_tool_call`, the whole jail implementation — `JAIL_READY_TIMEOUT`,
`WORKSPACE_JAIL_TOOLS`, `JailedWorkspaceSandboxProvisioner` (with its tool resolution), `impl WorkspaceSandboxProvisioner for …`, `start_jail_channel`,
`prepare_jail_tree`, `canonical`, `canonical_exec`, `InJailChannel`, `JailedWorkspaceSandbox`,
`impl WorkspaceSandbox for …`, `impl Drop for …`, `exchange_in_jail_tool_call`.

**Operation:** `extract_module --to_file`, name `jailed_workspace_sandbox`, `reexport: "named"`.

**Estimate (re-derive):** the seam was ~263 lines at 571; with the provisioner's tool resolution it is
larger, and the parent lands well under 500. A single seam clears the budget.

**Verified:** this plan passes a plain (non-deep) `check` — `no findings`. It has **not** been proven
by `check --deep`, which is the real gate.

## Constraints a later session must know

Line numbers above are approximate (taken at 684 production lines, 2026-11-02). Re-derive them with `restructure anchors`; never carry hand-written coordinates across a
file edit.
