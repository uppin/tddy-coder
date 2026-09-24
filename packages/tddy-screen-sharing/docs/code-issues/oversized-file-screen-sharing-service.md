# oversized-file: screen_sharing_service.rs

**Location:** `packages/tddy-screen-sharing/src/screen_sharing_service.rs`
**Category:** oversized-file
**Detected:** 2026-09-24 — `/pr-wrap` step 3.5 file-length gate on #509 (`#keyring` 2/9), production lines counted to the first `#[cfg(test)]`
**Metrics:** **967 production lines** of 2,176 total (`#[cfg(all(test, unix))]` at line 968; 1,209 test lines) · budget 500 · **1.9× over**
**Thresholds breached:** length 967 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seam table is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-24 | 967 | first detection. The size predates #509 (967 at merge-base `4e7157d2`). #509 changed 24 lines (12+/12−), all inside `mod tests`: `Request::new` → `Request::direct` in the request construction. No production line changed, so it did not grow the file |

## What would close it — candidate seams (not proven)

| Seam | Items | Out |
|---|---|---|
| A | bridge lifecycle, which is inherent and moves without stubs: `terminate_bridge`, `require_key`, `prompt_for_desktop_password`, `try_spawn_bridge`, `spawn_bridge` (L179–374) | ~195 |
| B | free bridge-config and naming functions: `PreparedBridge`, `prepare_bridge`, `build_bridge_spawn_config`, the `*_bridge_key` / `host_bridge_identity` / `screenshare_track_name` names, `browser_livekit_url`, `host_livekit_room`, `desktop_prompt_subject`, `decrypt_desktop_password`, the target-to-proto conversions (L377–622) | ~245 |
| C | host-target RPC bodies from the `ScreenSharingService` impl: `list_host_targets`, `add_host_target`, `start_host_stream`, `stop_host_stream` (L632–773) | ~140 |
| — | stays: `BridgeSpawnConfig`, `HostScope`, struct, builders and `require_*` helpers (L35–178), plus the vault-target RPCs (L774–961) | — |

From this first reading, A+B take it to about 525 lines, so Seam C (bodies moved behind delegating stubs) is also needed. The 1,209-line inline test module is a separate question and does not count against the budget. Prove the seams with `restructure check --deep` before applying them.

**Stack constraint:** `#keyring` dependents #510–#516 are still open, so no split lands until that stack does. `#keyring` 7/9 (#514) touches this file, so a split now would conflict with it.
