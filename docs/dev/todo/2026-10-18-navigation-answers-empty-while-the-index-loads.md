# 2026-10-18 — Navigation answers empty while the index is still loading

**Category:** Future enhancement
**Source:** `code-pane-index-status-strip` changeset ([#652](https://github.com/uppin/tddy-coder/pull/652))

## What is left

`tddy-index-daemon`'s `Definition`, `References` and `Hover` (`navigation.rs`) ask rust-analyzer as
soon as `WorkspaceIndex::client_for` hands out a live server, which happens long before its crate
graph is loaded. rust-analyzer answers those requests empty, so the Code pane shows "No definition
found" — indistinguishable from code that genuinely does not resolve.

## What would close it

Consult the root's `GraphLoad` latch (`graph.rs`, the one `Warm` waits on) before answering, and
report the loading state instead of an empty list — an `index_state` field on the three responses
(through `code_index.proto` and `code_navigation.proto`), which the pane's notice reads as "Index still
loading". Waiting on the latch instead was weighed and rejected: a cold load of this workspace takes
minutes, and a click must not hang that long.

## Why it was left

The status strip makes the loading state visible beside the pane; changing the navigation contract is a
proto change across two services.
