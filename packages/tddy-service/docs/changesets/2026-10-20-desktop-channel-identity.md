# 2026-10-20 — Desktop channel identity on the wire

**Type:** Feature

`#host-desktops` 7/8 ([#659](https://github.com/uppin/tddy-coder/pull/659)). `host.proto`:
`HostRemoteDesktop.channel_id` (7), `kind` (8, `DesktopChannelKind`), `display` (9), `label` (10).
`screen_sharing.proto`: `ScreenSharingTarget.x11_display` (7) and `AddHostTargetRequest.x11_display`
(8), both `optional uint32`.
