# 2026-03-18 — Debug, Demo Worktree, Workflow Logging

- **WebRTC debug output** (superseded by log config): Previously `--webrtc-debug-output <path>` routed libwebrtc logs to a separate file; now use `log:` section with `selector: { target: "libwebrtc" }`.
- **Demo worktree skip**: When backend is stub (tddy-demo), acceptance-tests uses output_dir directly; no git fetch or worktree creation.
- **Workflow failure logging**: Workflow failures logged at error level for visibility in debug output.
- **VirtualTui debug logs**: Input, keys, mouse, resize, render, frame sent at debug level for remote TUI troubleshooting.
- **web-dev**: Passes CLI args to daemon binary.
- **Packages**: tddy-core (log_backend, tdd_hooks, presenter), tddy-coder (Args, init_tddy_logger), tddy-tui (virtual_tui), tddy-web (mobile keyboard overlay).
