# 2026-10-21 — Desktop sign-in protos

**Type:** Feature

`#host-desktops` 8/8 ([#660](https://github.com/uppin/tddy-coder/pull/660)). `screen_sharing.proto` adds `InspectHostDesktop` (with its request
and response), `DesktopAuthMethod`, `DesktopCredentialAnswer` (the encrypted answer's plaintext),
and `domain` on `ScreenSharingTarget` (8) and `AddHostTargetRequest` (9). `host.proto` adds
`HOST_PROMPT_KIND_DESKTOP_CREDENTIALS = 3` and `HostPromptEvent.offered_auth_methods = 8`, which is
unpopulated pending removal. `HOST_PROMPT_KIND_DESKTOP_PASSWORD` is raised by nothing. TS is
regenerated in `tddy-web` and `tddy-rust-typescript-tests`.
