# 2026-09-16 — `IndexDaemonRegistry::connect` is public API with no caller

**Category:** Deferred refactor
**Source:** `2026-09-15-warm-code-intelligence-daemon` changeset, M6

`tddy-daemon` can own the index daemon's lifecycle — lazy spawn, readiness with a per-tick
child-death check, restart on exit, idle stop, cancellation on shutdown
(`packages/tddy-daemon/src/index_daemon.rs`). It does not *use* it: nothing in `tddy-daemon` calls
`get_or_spawn` or `connect`.

That is coherent as far as it goes. The daemon's job is to have the process running; `tddy-tools`
dials `TDDY_INDEX_SOCKET` directly, so no proxy is needed. But it leaves `connect()` as public API
with no in-crate caller and **no acceptance test on its success leg** — the only fixture that could
bind a real gRPC-over-UDS socket would be a fixture `[[bin]]`, which
[2026-09-10-the-execute-tool-stdio-fixture-bin-forces-three-dev-deps-into-dependencies.md](./2026-09-10-the-execute-tool-stdio-fixture-bin-forces-three-dev-deps-into-dependencies.md)
records as debt not to re-create. Its failure leg *is* tested, and the dialable path is covered from
both ends by the index daemon's own transport suite and `tddy-tools`' client suite.

## Two ways to close it, and they point in opposite directions

- **Give it a caller.** The daemon serving code intelligence to the web or to sessions is the obvious
  one, and would make the registry's lifecycle work earn its keep rather than duplicate what the
  developer-facing script already does.
- **Drop it** until there is one, leaving `get_or_spawn` / `reap_idle` / `shutdown` and removing the
  dial. Less API, and the client already knows how to dial.

Also worth knowing before either: the registry is constructed inside `runtime.rs`'s
`if let Some(user_resolver) = …` block, beside `tasks.lsp_idle_reaper`, so a daemon with no user
resolver assembles no index daemon — a second condition beyond the `index_daemon:` config section
being present.
