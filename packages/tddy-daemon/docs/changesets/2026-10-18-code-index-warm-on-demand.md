# 2026-10-18 — Index-daemon registry keep-alive; CodeSession wiring
**Type:** Feature

`IndexDaemonRegistry::connect_if_running` (dials only a running process, does not count as use),
`keep_alive` (refreshes the running process's idle timer, starts nothing) and `keep_alive_every` (a third
of `idle_timeout_secs`, clamped to 100 ms–60 s), implementing `IndexChannelSource`. `runtime.rs` drops
the `SessionIndexProgress` wiring (−9). Tests: `code_session_acceptance.rs` (19, new),
`code_index_warmup_acceptance.rs` rewritten (11), shared `tests/support/mod.rs`.

Code issues re-measured: `oversized-file-runtime` 1,831 → 1,822; `complexity-runtime-build` `build`
1,056 → 1,047.

Docs: [code-navigation-service.md](../code-navigation-service.md), [daemon-endpoint.md](../daemon-endpoint.md).
Cross-package entry: [2026-10-18-code-index-warm-on-demand.md](../../../../docs/dev/changesets/2026-10-18-code-index-warm-on-demand.md).
