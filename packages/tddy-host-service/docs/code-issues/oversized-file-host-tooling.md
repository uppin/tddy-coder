# oversized-file: host_tooling.rs

**Location:** `packages/tddy-host-service/src/host_tooling.rs`
**Category:** oversized-file
**Detected:** 2026-10-15 — `/pr-wrap` step 3.5 file-length gate on #622, production lines counted to the first `#[cfg(test)]`
**Metrics:** **775 production lines** · budget 500 · **1.55× over**
**Thresholds breached:** length 775 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — machine-measured count only

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-15 | 775 | first detection. 759 at the merge-base; #622 added 16 (`gh_auth_status_args`, per-hostname `probe`). Split deferred with the developer's consent |

## What would close it

Not designed. Split the probe implementations (gh, ssh-agent, others) from the probe trait and argv builders; prove seams with `restructure check --deep` first.
