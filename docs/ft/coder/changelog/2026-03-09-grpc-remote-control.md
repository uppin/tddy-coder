# 2026-03-09 — gRPC Remote Control

- **--grpc option**: tddy-coder and tddy-demo accept `--grpc [PORT]` (e.g. `--grpc 50052`). When provided, starts a tonic gRPC server alongside the TUI. Omit port to use default 50051.
- **Debug area**: Shown only when `--debug` is enabled; hidden otherwise.
- **Bidirectional streaming**: Clients connect via `Stream` RPC; send `UserIntent`s as `ClientMessage`, receive `PresenterEvent`s as `ServerMessage`.
- **tddy-grpc package**: New package with proto definition, TddyRemoteService, conversion layer. Depends on tddy-core.
- **Presenter event bus**: Presenter emits `PresenterEvent`s to optional broadcast channel; gRPC service subscribes and streams to clients.
- **External intents**: TUI event loop drains optional `mpsc::Receiver<UserIntent>`; gRPC forwards client intents to this channel.
- **Use case**: Programmatic control of TUI (e.g., E2E tests, automation) analogous to Selenium for web UIs.
