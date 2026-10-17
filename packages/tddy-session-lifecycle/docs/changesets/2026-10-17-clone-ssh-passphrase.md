# 2026-10-17 — A clone asks for an SSH key passphrase

**Type:** Feature

`DaemonSessionHost::with_askpass_endpoint` installs the daemon's `AskpassEndpoint` on the host, whence it reaches `LaunchSessions`. See [clone askpass](../../../tddy-host-service/docs/clone-askpass.md#wiring).
