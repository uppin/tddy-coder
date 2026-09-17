# 2026-09-16 — `IndexDaemonRegistry::connect` is public API ahead of its caller

**Category:** Deferred refactor
**Source:** `2026-09-15-warm-code-intelligence-daemon` changeset, M6

`tddy-daemon` owns the index daemon's lifecycle — lazy spawn, readiness with a per-tick child-death
check, restart on exit, idle stop, cancellation on shutdown
(`packages/tddy-daemon/src/index_daemon/`). It does not *use* it yet: nothing in `tddy-daemon` calls
`get_or_spawn` or `connect`.

That is deliberate — the lifecycle is ahead of its consumer, which lands in a future PR — but it has
a consequence worth recording: `connect()` is public API with no in-crate caller and **no acceptance
test on its success leg**. Its failure leg *is* tested, and the dialable path is covered from both
ends by the index daemon's own transport suite and `tddy-tools`' client suite, so the gap is the
registry dialling a live socket and nothing else. Closing it in isolation would need a fixture that
binds a real gRPC-over-UDS socket, and the only way to get one is a fixture `[[bin]]`, which
[2026-09-10-the-execute-tool-stdio-fixture-bin-forces-three-dev-deps-into-dependencies.md](./2026-09-10-the-execute-tool-stdio-fixture-bin-forces-three-dev-deps-into-dependencies.md)
records as debt not to re-create. The consumer's own acceptance test will cover it instead.

## The open question is which consumer

Two candidates, and the lifecycle work earns its keep either way:

- **the daemon serving code intelligence to the web** — queries arriving over `/rpc` from the
  dashboard;
- **the daemon serving it to sessions** — an agent in a session asking for the same index rather
  than each one starting its own.

`tddy-tools` is not the answer: it dials `TDDY_INDEX_SOCKET` directly, so it needs no proxy through
the daemon.

Also worth knowing before either: the registry is constructed inside `runtime.rs`'s
`if let Some(user_resolver) = …` block, beside `tasks.lsp_idle_reaper`, so a daemon with no user
resolver assembles no index daemon — a second condition beyond the `index_daemon:` config section
being present.
