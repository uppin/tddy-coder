# Changeset: Telegram Presenter RPC observer (2026-04-04)

## Scope

- **Proto**: `PresenterObserver.ObserveEvents` (server-streaming `ServerMessage`); `ServerMessage` extended with `backend_selected` for `PresenterEvent::BackendSelected`.
- **tddy-service**: `PresenterObserverService` bridging `broadcast::Sender<PresenterEvent>` to the stream; `event_to_server_message` maps `BackendSelected`.
- **tddy-coder**: Register `PresenterObserverServer` on the daemon gRPC router when LiveKit/presenter path is active (same `event_tx` as `Presenter`).
- **tddy-daemon**: `SpawnResult.grpc_port`; `TelegramSessionWatcher::on_server_message`; `TeloxideSender` + `InMemoryTelegramSender`; `telegram_session_subscriber` task after spawn; startup/shutdown messages with graceful HTTP shutdown.

## Packages

`tddy-service`, `tddy-coder`, `tddy-daemon`

## Verification

- `cargo test` workspace; daemon unit tests with `InMemoryTelegramSender` (no network).
- Manual: enable telegram, start session, confirm notifications.
