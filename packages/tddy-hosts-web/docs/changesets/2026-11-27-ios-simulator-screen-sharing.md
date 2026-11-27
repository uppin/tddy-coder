# 2026-11-27 — iOS Simulator channels, portrait fit, stream stats (PR #1060)

**Type:** Feature

`HostRowRemoteDesktop` renders `IOS_SIMULATOR` channels ("Simulator booted") and is addressed by
the host's routing id (`addressInstanceId`). `HostDesktopOverlay` attaches a simulator by `deviceId`
on `127.0.0.1:0` and starts it without inspection or sign-in. `ScreenSharingOverlay` sizes the
picture to the desktop's aspect and shows a bitrate / size / fps readout. See
[hosts-screen.md](../hosts-screen.md).
