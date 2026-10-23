# oversized-file: index.rs — the warm index's roots, servers and routing

**Location:** `packages/tddy-index-daemon/src/index.rs`
**Category:** oversized-file
**Detected:** 2026-10-22 by the `/pr-wrap` file-length gate on #681 (`#reshape` 22/24)
**Metrics:** **605 production lines** (2026-10-22, counted to the first `#[cfg(test)]`; 480 at the merge base) · budget 500
**Restructure:** required
**Status:** Open — split deferred: stack-shared
**Claimed by:** none

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-22 | 605 | #681 (`#reshape` 22/24, `extension-language-server`): +125 over the 480 at its base `feature/reshape/backend-fn-sizes`. `Route` / `ServedFile`, `route_of`, `client_for_route`, the per-root `languages` set and `holds_a_server_for` — the routing of an extension language's file to its own server (R7) |

## Why it was not split in #681

The `/pr-wrap` stack-overlap stop: `extension-outline` (#679, K20), `extension-code-actions` (K23) and
`extension-wasm-command` (K24) also edit this file, so a split here would turn each of their diffs into a
conflict. Deferred to a follow-up after the `#reshape` stack lands, as the stack's other deferred splits are
(the `#reshape` sweep node).

## What would close it

A `routing.rs` child taking what #681 added — `Route`, `ServedFile`, `WorkspaceIndex::route_of`,
`client_for_route` and `holds_a_server_for` (about 130 lines) — through `tddy-tools restructure`
(`extract_module` to a file, then `move_impl_members` for the three `WorkspaceIndex` methods), which
takes the file back to about 475.
