# Changeset: Telegram session control library (State B)

**Date**: 2026-04-05  
**Status**: Documentation complete (library + feature docs)  
**Type**: Feature (daemon library + product documentation)

## Affected areas

- `docs/ft/daemon/` — **`telegram-session-control.md`**, **`telegram-notifications.md`**, **`changelog.md`**
- `docs/dev/` — **`changesets.md`**, this WIP file
- `packages/tddy-daemon/src/` — **`telegram_session_control.rs`**, **`telegram_notifier.rs`** (`InMemoryTelegramSender`), **`tests/telegram_session_control_integration.rs`** (implementation; package `docs/` updates deferred to a dedicated package-doc pass per repo workflow)

## Summary

The **`telegram_session_control`** module provides inbound-oriented parsers, chunking, presenter-byte mapping, and a test harness that writes **`changeset.yaml`** and uses **`InMemoryTelegramSender`** with optional inline keyboard capture. Outbound session notifications remain in **`telegram_notifier`**. Feature documentation describes the split and the harness contract without delta phrasing.

## References

- [telegram-session-control.md](../../ft/daemon/telegram-session-control.md)
- [telegram-notifications.md](../../ft/daemon/telegram-notifications.md)
- [daemon changelog](../../ft/daemon/changelog.md)
