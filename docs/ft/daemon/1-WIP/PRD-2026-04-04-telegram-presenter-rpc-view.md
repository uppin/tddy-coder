# PRD: Telegram notifications from Presenter events (RPC observer)

## Goal

When `telegram` is enabled in daemon config, operators receive Telegram messages for:

- Daemon process start and graceful shutdown
- Workflow-relevant `PresenterEvent` traffic from each spawned `tddy-coder --daemon` child, delivered by the daemon subscribing to a **server-streaming gRPC observer** on the child’s gRPC port (not by polling `.session.yaml` alone).

## User-visible behavior

1. **Boot / shutdown**: Plain-text messages `tddy-daemon started` and `tddy-daemon stopped` to all configured `chat_ids` when Telegram is enabled.
2. **Per session**: After `StartSession` / `ResumeSession` spawn succeeds, the daemon connects to `127.0.0.1:{grpc_port}` and consumes `ObserveEvents`, mapping streamed `ServerMessage` events to concise notifications (state transitions, workflow completion, goal started, backend selected).
3. **Spam control**: Repeated identical logical events for a session do not generate duplicate Telegram sends.

## Non-goals

- Cross-daemon routing of Telegram (unchanged).
- Replacing the existing metadata-based watcher API entirely (library may support both paths during transition).

## Configuration

Unchanged: top-level `telegram` block in daemon YAML (`enabled`, `bot_token`, `chat_ids`).

## Acceptance

Aligned with implementation plan: observer proto + server on child, `grpc_port` on spawn result, daemon gRPC client + watcher mapping, in-memory sender for tests, production path via `TelegramSender` + teloxide.

## Related

- [telegram-notifications.md](../telegram-notifications.md) (product overview)
- Dev changeset: `docs/dev/1-WIP/2026-04-04-telegram-presenter-rpc-view.md`
