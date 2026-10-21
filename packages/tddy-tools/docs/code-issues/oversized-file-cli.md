# oversized-file: cli.rs

**Location:** `packages/tddy-tools/src/cli.rs`
**Category:** oversized-file
**Detected:** 2026-10-21 by `/pr-wrap` step 3.5 file-length gate, in #690 (`#satellite` 15/15)
**Metrics:** **895 production lines** (the `#[cfg(test)]` module opens at L896; 910 total) · budget
500 · **1.8× over**
**Thresholds breached:** length 895 > 500
**Restructure:** required — `extract_module --to_file`, `/code-restructuring` territory
**Status:** Open — unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-21 | 895 | first detection. 895 on the merge-base and 895 after #690: the module was re-parented from the `tddy-tools` binary root into the library (three import lines repointed `tddy_tools::` → `crate::`), not grown. Alert-only row of the gate |

## What the tool found

One file holds every clap `Args` struct of the relay subcommands and their `run_*` / `relay_*`
bodies: `submit`, `ask`, `spawn-conversation`, `list-tools`, `call-tool`, `transition`,
`list-schemas`, `persist-changeset-workflow`, `set-session-context`, `list-actions`,
`invoke-action`, `get-schema`. Each `relay_*` has a `cfg(unix)` / `cfg(not(unix))` pair.

## Why it matters here

The subcommands share almost nothing beyond `read_input` and the `output_*` printers, so a change
to one subcommand is read against eleven others.

## What would close it

Split by subcommand family with `extract_module --to_file` — the workflow relay (`submit`, `ask`,
`transition`, `spawn-conversation`), the session tools (`list-tools`, `call-tool`), the schema and
changeset commands, and the session actions (`list-actions`, `invoke-action`) — leaving `cli.rs`
as the shared helpers and a facade. `complexity-cli-run-call-tool.md` is in the same file; do that
`extract_method` first or in the same plan.
