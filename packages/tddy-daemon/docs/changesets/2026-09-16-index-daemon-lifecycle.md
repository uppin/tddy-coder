# 2026-09-16 — The index daemon's lifecycle

**Type:** Feature

The daemon can own a `tddy-index-daemon` process: lazily spawned on the first request that needs it,
supervised, restarted when it exits, stopped when it has gone idle, and cancelled rather than
orphaned on shutdown. Off unless the configuration carries an `index_daemon:` section, so every
existing deployment is byte-identical; it also requires a user resolver, like every other service the
daemon assembles.

Ownership is split deliberately: the index lives in its own process — rust-analyzer is a
multi-gigabyte resident child, and a headless deployment that never restructures anything should not
pay for it — while the daemon decides when it runs. It also needs a different language-server
handshake from the one the daemon's own registry advertises, and a handshake is fixed at spawn.

Built in the shape `tddy_lsp::LspRegistry` established one level up: look up, take a spawn gate,
look again because the previous holder may have started the very process you came for, then start.
Readiness is the socket appearing, with a child-death check on every tick, so a child that dies
before binding fails fast with its decoded exit reason instead of timing out — and the socket, not the
child's log line, because reading the line would depend on the child's inherited log level.

The child body follows `LspServerBody`, not the sandbox runner's: it registers the child pid for the
task registry's escalation, observes the exit by selecting on `child.wait()` rather than discovering
it later, and does a bounded graceful shutdown before killing. `main.rs` stops it explicitly wherever
it stops CLI sessions, and before aborting the background tasks — aborting the reaper first is how the
sandbox path orphans a runner.

A dead predecessor's socket is cleared before a replacement binds, and that clearing is proven to
have happened by a `#[must_use]` witness the readiness wait takes as an argument. A mutation test
found the reason: with the clearing removed, all eleven tests still passed, because a stale socket
satisfied "the path exists" and a child that died immediately read as serving.
