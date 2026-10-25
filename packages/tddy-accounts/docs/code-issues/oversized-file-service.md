# oversized-file: service.rs

**Location:** `packages/tddy-accounts/src/service.rs`
**Category:** oversized-file
**Detected:** 2026-10-25 — `/pr-wrap` step 3.5 file-length gate on #754 (`#start-parity` 5/9), production lines counted to the first `#[cfg(test)]`
**Metrics:** **772 production lines** · budget 500 · 1.5× over
**Thresholds breached:** length 772 > 500
**Restructure:** not designed
**Status:** Open — unclaimed. Already over budget before #754; #754 touched it and did not grow it
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seams are a first reading

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-25 | 772 | first detection. `#start-parity` 5/9 (#754): 772 → 772 — `build_accounts_entry` now takes an `Arc` so the daemon can keep a handle on the instance the RPC serves; written to add no lines |

## What would close it — candidate seams (not proven)

| Seam | Items |
|---|---|
| A | the link flow (`Linking`, the `impl` block from `:478`, `store_link`, `state_only`, `pending`, `link_status_for`) → `service/linking.rs` |
| B | the provider-account answers (`route_option`, `grouped_by_provider`, `requested_account`, `refusal_of_provider_account`) |
| C | the sync summary (`summary_of`, `sync_status_proto_of`) |

Prove the seams with `restructure check --deep` before applying them.
