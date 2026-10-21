# 2026-10-21 — the extracted desktop-start helpers carry long parameter lists and the inherited nesting

**Category:** Code quality
**Source:** `/analyze-clean-code` on `#host-desktops` 10/10, [#693](https://github.com/uppin/tddy-coder/pull/693)

#693 was a pure move, so its cleanups could not change a signature or a moved body. What it left:

| Function | File | Metric | Value |
|---|---|---|---|
| `start_bridge_for_desktop` | `packages/tddy-screen-sharing/src/screen_sharing_service/host_targets.rs` | parameters | 6 + `&self` (`req, operator, config, target, prepared, sign_in`) |
| `start_session_bridge` | `packages/tddy-screen-sharing/src/screen_sharing_service/session_stream.rs` | parameters / nesting | 5 + `&self`; the `if let` → `match` → `match` ladder it took over from `start_stream` nests six deep |
| `await_passphrase_answer` | `packages/tddy-host-service/src/service/host_keys.rs` | form | ends `let answer = match …; answer` (the assist's let-and-return; clippy does not flag it here) |

**Fix shape:** a small struct for the host-desktop start (`HostDesktopStart { req, operator, target, prepared,
sign_in }`) passed to `start_bridge_for_desktop`; flatten `start_session_bridge` with early `let … else`
returns. The nesting is the same ladder
[2026-10-20-session-start-without-a-bridge-succeeds.md](2026-10-20-session-start-without-a-bridge-succeeds.md)
changes (its no-bridge branches), so do both together: the flattening is where that behaviour change lands.
Return the `match` directly in `await_passphrase_answer`. None of these changes behaviour except the linked
entry's.
