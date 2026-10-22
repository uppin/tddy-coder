# 2026-10-22 — Muse Code in the New-session form and the Default-for control

**Type:** Feature

`#provider-route` 6/9 ([#667](https://github.com/uppin/tddy-coder/pull/667)). `SessionType` gains
`"muse-cli"`; the session-type toggle is one button per `SESSION_TYPE_OPTIONS` entry, ending with
**Muse Code** (`create-session-type-muse-cli`); the opencode-cli and muse-cli requests are one branch of
`buildStartSessionRequest`. `ProviderAccountDefaultsControl` offers **Muse Code** again, now that a
muse-cli session can start. Code issue `oversized-file-create-session-pane` re-measured: 832 lines,
unchanged. See [create-session provider](../create-session-provider.md).
