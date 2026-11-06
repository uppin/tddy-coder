# oversized-file: cli_harness_jail.rs

**Location:** `packages/tddy-agent-launch/src/cli_harness_jail.rs`
**Category:** oversized-file
**Detected:** 2026-11-06 by `/pr-wrap` on PR #825 (`#agent-tools` 19/19), measured on `origin/feature/agent-tools/telegram-spec-launch` and HEAD at #825's wrap
**Metrics:** **591 production lines** (counted to the first `#[cfg(test)]`) · budget 500 · **1.18× over**
**Thresholds breached:** length 591 > 500
**Status:** Open — regressed 2026-11-06 by #825 (582 → 591) — unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-06 | 582 → 591 | first detection — the file was already over budget at the base with no record; #825 grew it by 9: `agent_tddy_tools` (the host-resolved `tddy-tools` an agent's tool file depends on, replacing the daemon.yaml `tddy_tools_path` reads) and the `HomeSection::Path` arm of the jail home. Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer |

## What would close it

Split by seam with `code-restructuring`: the jail toolset and its closure (`closure_defs`, `withdrawn_tools`, `JailToolset`, `managed_prompt`) into `cli_harness_jail/toolset.rs`, and the legacy runner path (`resolve_cli_jail_binaries`, `CliJailBinaries`, `CliRunnerArgv`, `cli_harness_runner_argv`, `sandboxed_runner_model`, `model_argument`) into `cli_harness_jail/runner.rs` — the latter goes with the `CliHarness` table (`docs/dev/todo/2026-11-03-harness-clis-legacy-cli-harness-removal.md`).
