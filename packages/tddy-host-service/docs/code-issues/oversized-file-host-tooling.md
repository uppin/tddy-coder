# oversized-file: host_tooling.rs

**Location:** `packages/tddy-host-service/src/host_tooling.rs`
**Category:** oversized-file
**Detected:** 2026-10-15 — `/pr-wrap` step 3.5 file-length gate on #622, production lines counted to the first `#[cfg(test)]`
**Metrics:** **806 production lines** · budget 500 · **1.61× over** (775 before #659)
**Thresholds breached:** length 806 > 500
**Restructure:** not designed
**Status:** Open — regressed 2026-10-20 (775 → 806 in #659) — **unclaimed**
**Verified:** ⚠ **not hand-verified** — machine-measured count only

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-15 | 775 | first detection. 759 at the merge-base; #622 added 16 (`gh_auth_status_args`, per-hostname `probe`). Split deferred with the developer's consent |
| 2026-10-20 | 775 | #host-desktops 4/8 (hosts-mount): unchanged. The cache lives in `host_tooling_cache.rs` and its call site in `service.rs`, so this file did not grow |
| 2026-10-20 | 806 | #host-desktops 7/8 (#659, x11-channels): +31. The X11 enumeration is its own module (`x11_displays.rs`, 144 production lines); this file gained the probe's `x11_channels` field, the `listing_x11_displays_for` setter, a `with_x11_channels` delegate and the two call sites. Recorded per the changeset's ⚠ DURING verdict, not split; #660 does not touch this file |

## What would close it

Not designed. Split the probe implementations (gh, ssh-agent, others) from the probe trait and argv builders; prove seams with `restructure check --deep` first.
