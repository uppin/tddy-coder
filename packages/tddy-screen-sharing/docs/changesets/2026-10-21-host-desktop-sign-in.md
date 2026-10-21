# 2026-10-21 — A host desktop signs in with a remembered or prompted credential

**Type:** Feature

`#host-desktops` 8/8 ([#660](https://github.com/uppin/tddy-coder/pull/660)). `host_desktop_sign_in.rs` provides `inspect_host_desktop`, which
runs the bridge with `--inspect`, answers `NONE` for an X11 display, and maps failures to
`Unavailable` (bridge reason) or `DeadlineExceeded` (35 s). It also provides
`decrypt_desktop_credential_answer`, `remembered_account` (`host:<instance>:<target>`) and
`RememberedDesktopCredentials` (`usable`, `can_remember`, `remember`, `mark_refused`).
`desktop_credential_for` returns `SignIn { answer, remembering: Nothing | AlreadyRemembered |
AfterStart }`, and `start_signed_in_bridge` settles the vault around the spawn: it marks a refused
remembered credential refused, and writes a Remember only after the start. `BridgeSpawnConfig`
gains `domain`. `SessionVaultTargetStore::list` skips `host:` accounts. `prost` is a normal
dependency (0.13, already in the lockfile). Tests: `tests/host_desktop_sign_in_acceptance.rs` (15).
