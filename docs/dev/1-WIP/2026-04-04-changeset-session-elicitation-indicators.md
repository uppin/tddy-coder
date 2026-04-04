# Changeset: Session elicitation indicators (State B)

**Date**: 2026-04-04  
**Status**: Complete  
**Type**: Feature (product + technical documentation)

## Affected packages

- `docs/ft/daemon/` — feature documentation
- `docs/ft/web/` — feature documentation
- `docs/dev/` — cross-package changeset index
- `packages/tddy-core/docs/changesets.md`
- `packages/tddy-daemon/docs/` — `telegram-notifier.md`, `changesets.md`
- `packages/tddy-service/docs/changesets.md`
- `packages/tddy-web/docs/changesets.md`

## Summary

Operators see when a session blocks on human input in two places: Telegram messages for presenter **`ModeChanged`** elicitation-class modes, and the web Connection session list via **`SessionEntry.pending_elicitation`** aligned with **`.session.yaml`**. Feature documentation describes behavior, RPC field, and UI affordances without delta phrasing.

## References

- [telegram-notifications.md](../../ft/daemon/telegram-notifications.md)
- [web-terminal.md](../../ft/web/web-terminal.md)
- [telegram-notifier.md](../../../packages/tddy-daemon/docs/telegram-notifier.md)
