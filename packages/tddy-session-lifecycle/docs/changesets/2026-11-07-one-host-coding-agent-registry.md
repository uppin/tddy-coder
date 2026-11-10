# 2026-11-07 — The session host is constructed over its coding-agent registry

**Type:** Refactor

`#agent-tools` 22/28 ([#830](https://github.com/uppin/tddy-coder/pull/830)).

- `DaemonSessionHost::new(…, cli_manager, coding_agents: Arc<HostCodingAgents>)`; the field is non-optional. `coding_agents()` returns `&Arc<HostCodingAgents>`; `with_coding_agents` replaces it; `with_coding_agents_if_none` is deleted.
- The three empty-registry fallbacks are deleted (`daemon_rpc_handler`'s `EMPTY_CODING_AGENTS`, `svc_resolve_os_user`, `svc_activity_ports`); `TmuxMainReattach` holds the `Arc`.
- The five room-port builders move to `svc_host_builders/svc_room_builders.rs` (engine moves): `svc_host_builders.rs` 510 → 465 production lines; `oversized-file-svc-host-builders` closed at **465**.
- Fixtures pass `test_util::builtin_coding_agents()` or `no_coding_agents()` at construction (~117 call sites).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-07-one-host-coding-agent-registry.md).
