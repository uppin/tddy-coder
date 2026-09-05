# 2026-07-11 — Linux daemon-client mode

**Type:** Feature

on Linux the app talks to a running `tddy-daemon` over tonic UDS (`MintLocalToken` → `StartSession` with `repo_path`/`claude_args` → `StreamSessionTerminalIO` PTY proxy) instead of spawning the jail in-process; reuses the `bridge.rs` terminal front-end; adds `--daemon-socket`; darwin dep macOS-gated; macOS Seatbelt path unchanged. Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#291](https://github.com/uppin/tddy-coder/pull/291) (draft; **not run end-to-end**). (tddy-sandbox-app, tddy-service, tddy-sandbox-runner)
