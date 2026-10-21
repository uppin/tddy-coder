# 2026-10-20 — the start paths of screen sharing are over the function-length budget

**Category:** Code quality
**Source:** `#host-desktops` 6/8 (`feature/host-desktops/bridge-ready`, PR #658), `/analyze-clean-code`

- `packages/tddy-screen-sharing/src/screen_sharing_service.rs` `start_stream`: 72 lines (63 before
  #658, which added the wait for the bridge's report and the size it reports). Over the 60-line
  must-refactor line. The target lookup → password → `BridgeTarget` → spawn ladder is the natural
  helper (`start_session_bridge`), and it is also where the no-bridge branches of
  [2026-10-20-session-start-without-a-bridge-succeeds.md](2026-10-20-session-start-without-a-bridge-succeeds.md)
  sit.
- `packages/tddy-screenshare/src/bridge.rs` `start_bridge`: 58 lines (needs attention). It was split
  out of `run_bridge` by #658, which brought `run_bridge` down from 68 to 56. The LiveKit join and
  track publish is the second helper.
- `packages/tddy-screen-sharing/src/screen_sharing_service/host_targets.rs` `start_host_stream`:
  67 lines, the same as before #658 (pre-existing).
- **Why deferred:** `screen_sharing_service.rs` and `bridge.rs` are rewritten by `#host-desktops`
  7/8 (#659) and 8/8 (#660), stacked on #658. Extracting a method in either one now would conflict
  with both. `/pr-wrap` step 4 forbids restructuring a stack-shared file.
- **Fix shape:** once the stack lands, extract the helpers named above. Do it together with the
  no-bridge entry, because the same ladder changes there.
