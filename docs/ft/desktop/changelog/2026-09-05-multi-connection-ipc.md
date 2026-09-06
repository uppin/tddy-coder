# 2026-09-05 — The desktop IPC bridge carries many addressed connections

A page in the desktop app can hold several IPC connections at once, each addressed at what it
reaches, where before it held exactly one and that one reached the daemon. Opening a connection no
longer disturbs the others, closing one leaves the rest serving, and a page that stops reading one
channel stalls only that connection.

A connection names its target when it opens — the daemon, or one session by id — and a third command,
`tddy_rpc_disconnect`, gives one back. Sessions come and go far more often than pages do, so a
connection that cannot be released would leak a host-side peer on every attach. Everything a page
opened is released as a replacing page commits, which one connection slot used to do implicitly.

The addressing is real per connection: each gets its own epoch, engine peer and backpressure. What
every target resolves to today is the same daemon roster, because the embedded daemon serves
session-scoped RPC itself and routes it by what the request names — so a call for a session the daemon
does not have is answered by the daemon rather than refused at connect. The application does not yet
open session connections of its own; the dashboard still reaches everything over its daemon
connection, and the provider that opens one per attached session is separate work.

Nothing about the frame format, the request/response protocol or the transport choice changed, and no
LiveKit identity string can cross the IPC boundary — a connection's target is a closed set of two
kinds, not a string.

`./desktop-dev` also stopped resolving the workspace root to the checkout's parent, where
`dev.desktop.yaml` and the data directory do not exist.

See [Tddy desktop app (Tauri)](../tddy-desktop-tauri.md) and the implementation notes in
[addressed webview IPC connections](../../../../packages/tddy-desktop/docs/webview-ipc-connections.md).
