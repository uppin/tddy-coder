# oversized-file: remote_cli.rs

**Location:** `packages/tddy-tools/src/remote_cli.rs`
**Category:** oversized-file
**Detected:** 2026-10-25 — `/pr-wrap` step 3.5 on `#coagents` 6/8 (#725), production lines counted to the first `#[cfg(test)]` (the file has none, so every line counts)
**Metrics:** **509 production lines** · budget 500
**Thresholds breached:** length 509 > 500
**Restructure:** `move_item` — `/code-restructuring` territory
**Status:** Open — unclaimed; **deferred by developer decision** (2026-10-25: code issues in the `#coagents` stack are recorded for the sweep, not solved in place)
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable

## Measurement history

| Run | Production lines | Note |
|---|---:|---|
| 2026-10-25 | 357 → 509 | first detection; **+152** in #725: the `remote` wire became protobuf. `connect_post` is generic over the prost request and response types, each subcommand builds its typed request, and the section "Connect JSON of a decoded response" (`ConnectJson` and its `impl`, `list_tools_json`, `tool_def_json`, `start_session_json`, `branch_conflict_json`, `branch_session_json`, `connect_session_json`, `execute_tool_json`, ~110 lines) prints each answer. This PR pushed the file past 500 |

## What would close it

Move the "Connect JSON of a decoded response" section into a module of its own:
one `move_item` over `tddy_tools::remote_cli::{ConnectJson, <ConnectJson>, list_tools_json,
tool_def_json, start_session_json, branch_conflict_json, branch_session_json, connect_session_json,
execute_tool_json}` (they are contiguous; `restructure anchors --items` emits the anchor) with
`"name": "connect_json"`, `"to": "tddy_tools::remote_cli"`, `"reexport": "none"`, which brings the file
to about 400 lines. The section's leading `//` comment becomes the new module's `//!` doc.
