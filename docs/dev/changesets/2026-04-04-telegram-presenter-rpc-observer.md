# 2026-04-04 — Telegram Presenter RPC observer

**Type:** Feature

Proto **`PresenterObserver.ObserveEvents`**, **`ServerMessage.backend_selected`**; **`tddy-service`** **`PresenterObserverService`**; **`tddy-coder`** registers observer on daemon gRPC; **`tddy-daemon`** **`SpawnResult.grpc_port`**, **`TelegramDaemonHooks`** / **`telegram_session_subscriber`**, **`TeloxideSender`** / **`InMemoryTelegramSender`**, lifecycle messages + graceful HTTP shutdown. (tddy-service, tddy-coder, tddy-daemon, docs)
