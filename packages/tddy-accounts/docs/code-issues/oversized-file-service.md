# oversized-file: service.rs

**Location:** `packages/tddy-accounts/src/service.rs`
**Category:** oversized-file
**Detected:** 2026-10-25 — `/pr-wrap` step 3.5 file-length gate on #754 (`#start-parity` 5/9), production lines counted to the first `#[cfg(test)]`
**Metrics:** **790 production lines** · budget 500 · 1.6× over
**Thresholds breached:** length 790 > 500
**Restructure:** not designed
**Status:** Open — unclaimed. Already over budget before #754; #916 grew it 772 → 790 and deferred the split to the follow-up named in the measurement history
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seams are a first reading

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-25 | 772 | first detection. `#start-parity` 5/9 (#754): 772 → 772 — `build_accounts_entry` now takes an `Arc` so the daemon can keep a handle on the instance the RPC serves; written to add no lines |
| 2026-11-16 | 790 | `#remote-shell` 2/7 (#916): 772 → 790 — one delegating `connect_coder_account` arm, the `coder` field and two imports; every Coder helper lives in `coder_account.rs`. Not split here: that PR's boundaries forbid it and the file is inherited by every later `#remote-shell` node. The deferred split is `docs/dev/todo/2026-10-21-tddy-accounts-service-rs-over-file-budget.md` |

## What would close it — candidate seams (not proven)

| Seam | Items |
|---|---|
| A | the link flow (`Linking`, the `impl` block from `:478`, `store_link`, `state_only`, `pending`, `link_status_for`) → `service/linking.rs` |
| B | the provider-account answers (`route_option`, `grouped_by_provider`, `requested_account`, `refusal_of_provider_account`) |
| C | the sync summary (`summary_of`, `sync_status_proto_of`) |

Prove the seams with `restructure check --deep` before applying them.
