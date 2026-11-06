# oversized-file: test_util.rs

**Location:** `packages/tddy-session-lifecycle/src/test_util.rs`
**Category:** oversized-file
**Detected:** 2026-11-06 by `/pr-wrap` on PR #825 (`#agent-tools` 19/19), measured on `origin/feature/agent-tools/telegram-spec-launch` and HEAD at #825's wrap
**Metrics:** **531 production lines** (counted to the first `#[cfg(test)]`) · budget 500 · **1.06× over**
**Thresholds breached:** length 531 > 500
**Status:** Open — regressed 2026-11-06 by #825 (501 → 531) — unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-06 | 501 → 531 | first detection — #825 replaced `daemon_config_from_fixture_file` and `builtin_coding_agents_for` with `coding_agents_with_binaries` and its fixture host `AHostOfBinaries` (the built-in agents on a host that maps each spec's `binary.env` to a stub, with an empty `PATH`). Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer |

## What would close it

Split by seam with `code-restructuring`: the coding-agent fixtures (`builtin_coding_agents`, `coding_agents_with_binaries`, `AHostOfBinaries`) into `test_util/coding_agents.rs`, and the signed-credential host (`SignedTestCredentials`, `signing_test_host`) into `test_util/signing.rs`. Each leaves the module well under budget.
