# oversized-file: screen_sharing_service.rs

**Location:** `packages/tddy-screen-sharing/src/screen_sharing_service.rs`
**Category:** oversized-file
**Detected:** 2026-09-24 — `/pr-wrap` step 3.5 file-length gate on #509 (`#keyring` 2/9), production lines counted to the first `#[cfg(test)]`
**Metrics:** **537 production lines** (2026-10-19, #653; was 983 before the split, 967 before #626) of 1,890 total (`#[cfg(all(test, unix))]` at line 538) · budget 500 · **1.07× over**
**Thresholds breached:** length 537 > 500
**Restructure:** seams A and B applied in #653; seam C not done (engine gap, see below)
**Status:** Open, **narrowed** 2026-10-19 (983 -> 537) — **claimed**
**Claimed by:** #653 — `#host-desktops 1/8` (`feature/host-desktops/service-split`)
**Lands after:** nothing open — #626 (`#satellite 4/13`) and #625 merged 2026-10-18 (`dc84ebe8c`), which was the developer's gate for starting the split
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seam table is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-24 | 967 | first detection. The size predates #509 (967 at merge-base `4e7157d2`). #509 changed 24 lines (12+/12−), all inside `mod tests`: `Request::new` → `Request::direct` in the request construction. No production line changed, so it did not grow the file |
| 2026-10-07 | 967 | touched, unchanged: #514 (`#keyring` 7/9) deletes `require_key` and the key-cache plumbing and rewrites the target RPCs over `ScreenSharingTargetStore` (−10 lines), then adds `with_credential_vaults` (+10, a 7-line method, its doc line, and imports) — 967 production lines on `origin/master` and on HEAD. The file was not grown; seam A lost `require_key` |
| 2026-10-18 | 967 → 983 (+16) | #626 (`#satellite` 4/13): bridge tokens minted through `RoomTokenMinter`; the key and secret re-reads leave `build_bridge_spawn_config` (−12), `with_room_tokens`, the threaded parameter and `BRIDGE_TOKEN_TTL` come in. Grown; split deferred with the developer's consent — `docs/dev/todo/2026-10-17-satellite-1-left-files-over-the-500-production-line-budget.md` |
| 2026-10-19 | 983 → 537 (−446) | #653 (`#host-desktops` 1/8): seams A and B applied by `tddy-tools restructure` (`bridge_lifecycle`, `bridge_config`); seam C refused by the engine, not done |

## Result of #653 (2026-10-19)

| Seam | Done | How | Where it went |
|---|---|---|---|
| A | yes | `extract_module` (not `retarget_impl`: that changes an `impl`'s self type and refuses the same type, "there is nothing to retarget") over `terminate_bridge … spawn_bridge`, `require_target_store` included because it sits between them | `src/screen_sharing_service/bridge_lifecycle.rs` (243 lines) — nested under the service module |
| B | yes | `move_item` | `src/bridge_config.rs` (264 lines) |
| C | **no** | `extract_method` over the four host-target RPC bodies is refused under `#[async_trait]`: "offered: Extract into variable" | still in `screen_sharing_service.rs`; engine gap filed as `docs/dev/todo/2026-10-19-restructure-extract-method-refused-under-async-trait.md` |

**Remaining:** 537 production lines, 37 over budget. What is left is the struct and builders, `BridgeTarget`,
`HostScope`, `target_error_to_status`, the four host-target RPC bodies (about 140 lines, seam C) and the
session (vault) RPCs. Moving seam C's bodies would take it to roughly 400.

**Not hand-verified further:** the members moved by A and the items moved by B were widened to `pub(crate)`
by the engine (and `HostScope` by hand); that is the cost of the split, not narrowed back.

## Concurrent changes

- **#626** (`#satellite 4/13`, merged 2026-10-18) threaded a `RoomTokenMinter` through `prepare_bridge` /
  `build_bridge_spawn_config` (seam B). #653 moves that merged shape.
- **#658, #659, #660** (`#host-desktops` 6–8) edit `spawn_bridge` and the host-target RPCs after
  #653 has split them; they target the new modules, not this file.
