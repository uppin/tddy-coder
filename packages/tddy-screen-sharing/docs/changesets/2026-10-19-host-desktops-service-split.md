# 2026-10-19 — Screen-sharing service split into bridge, naming and host-target modules

**Type:** Architecture

`screen_sharing_service.rs` is split along the seams its code-issue record named, with no behaviour
change (`#host-desktops` 1/8, #653).

- **Bridge lifecycle** (`terminate_bridge`, `require_target_store`, `prompt_for_desktop_password`,
  `try_spawn_bridge`, `spawn_bridge`) moved to `screen_sharing_service/bridge_lifecycle.rs`, as
  `impl ScreenSharingServiceImpl` members, by `tddy-tools restructure` (`extract_module`).
- **Bridge configuration and naming** (`BridgeSpawnConfig`, `PreparedBridge`, `prepare_bridge`,
  `build_bridge_spawn_config`, the key, identity and track-name helpers, the LiveKit room and URL
  helpers, `decrypt_desktop_password`, the target conversions) moved to `bridge_config.rs`
  (`move_item`), as `#626` left them.
- **Host-target RPC bodies** moved to `screen_sharing_service/host_targets.rs` as `*_impl` members;
  the trait methods delegate. This seam was moved **by hand, with the developer's consent**,
  byte-identical to the originals: the engine's `extract_method` is refused inside an
  `#[async_trait]` impl.
- Items the moved code reaches across the new module boundaries became `pub(crate)`; no public path
  changed.

**Final measurement** — `screen_sharing_service.rs`, production lines to the first `#[cfg(test)]`:
983 → **421** (budget 500; 967 before #626, 1,777 lines in total); `bridge_config.rs` 264,
`bridge_lifecycle.rs` 243, `host_targets.rs` 160. The oversized-file record is closed.

**Verification** (scoped to `tddy-screen-sharing`): `./test -p tddy-screen-sharing` 49 passed, 0 failed
before and after; `cargo fmt --check` and `cargo clippy -p tddy-screen-sharing --all-targets -- -D
warnings` clean; no `//` comment lost across the move.

**Left open:** the `extract_method` refusal under `#[async_trait]` stays in the backlog
(`2026-10-19-restructure-extract-method-refused-under-async-trait`). Hand-edit markers
(`TODO(restructure)`) remain at the hand-made edits until it is closed.
