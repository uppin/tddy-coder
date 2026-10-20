# 2026-10-21 — Host desktops are asked for credentials, not a password

**Type:** Feature

`#host-desktops` 8/8 ([#660](https://github.com/uppin/tddy-coder/pull/660)). `PromptKind::DesktopCredentials` (wire `DESKTOP_CREDENTIALS`)
replaces `PromptKind::DesktopPassword`, which is removed. The proto value stays and is raised by
nothing. `HostPromptEvent.offered_auth_methods` is forwarded empty. See
[host-registry.md](../host-registry.md).
