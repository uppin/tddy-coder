# 2026-09-22 — The `PresenterEventSink` port

**Type:** Architecture

`#carve` 8/11 ([#494](https://github.com/uppin/tddy-coder/pull/494)). Cross-package entry:
[2026-09-22-carve-telegram.md](../../../../docs/dev/changesets/2026-09-22-carve-telegram.md).

`presenter_observer::{PresenterEventSink, SharedPresenterEventSink}` is where a workflow session's
presenter events go besides the notification bus. `tddy-session-lifecycle`'s connection service
holds it as an `Option`, and `tddy-telegram-control`'s `TelegramDaemonHooks` implements it. The
kernel ships no no-op implementation. It is the first port admitted here, under the rule in
[daemon-kernel.md](../daemon-kernel.md) § Ports.

`tests/telegram_extraction_shape.rs` (9 tests) proves delivery through the port with a recording
sink, and pins the extraction around it. `toml` is a new dev-dependency, already in `Cargo.lock`.
