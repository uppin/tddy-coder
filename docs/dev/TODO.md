# Development TODO

## Future Enhancements

### tddy-coder (source: subagent-tool-replacement changeset, 2026-07-02)

- **Extend subagent tool-replacement to the `--remote` path** — `packages/tddy-coder/src/remote.rs`'s `RemoteContextDir`/`REMOTE_APPENDIX` and `build_remote_allowlist` have no subagent concept at all today (no `SUBAGENT_TOOLS`, no `subagent_*` wiring in `run_remote`). Extending the tool-replacement mechanism there requires wiring subagent support into that path first — deferred to keep this changeset scoped to the sandbox/daemon managed path where subagents already exist.
- **Per-tool replacement policies** — the replaced-tool set is a flat list today (e.g. all of `Grep`, unconditionally). A future refinement could scope replacement (e.g. only for certain file types or path prefixes) rather than an all-or-nothing per-tool switch.

### tddy-vm / tddy-daemon (discovered while verifying the subagent-tool-replacement changeset, 2026-07-02)

- **`vm_service_acceptance.rs`'s `BuildVmImage` tests hang in the standard nix dev shell** — `build_vm_image_adapter_still_delivers_progress_messages`, `build_vm_image_streams_progress_messages`, and `vm_build_task_appears_in_registry_after_build_call` (`packages/tddy-daemon/tests/vm_service_acceptance.rs`) all assume `BUILDROOT_DIR` is unset so `run_buildroot_pipeline` (`packages/tddy-vm/src/build.rs:968-976`) takes the fast `STAGE_ERROR` path. `./dev`'s nix shell unconditionally exports `BUILDROOT_DIR` to a real Buildroot source tree, so these tests instead fall through to a real `make olddefconfig`/`make -j<nproc>` build (via Docker on macOS) — effectively hanging (or taking a very long time) rather than failing fast. Pre-existing, unrelated to any code in this changeset; not fixed here because the right fix (mock/stub the pipeline at a lower level, or have the dev shell not export `BUILDROOT_DIR` for test runs) needs a decision from whoever owns the VM-build feature. Workaround used during this changeset's verification: `cargo test -- --skip build_vm_image_adapter_still_delivers_progress_messages --skip build_vm_image_streams_progress_messages --skip vm_build_task_appears_in_registry_after_build_call`.

### tddy-sandbox-cgroups (source: finish-stdio-ipc-migration changeset, 2026-07-02)

- **Verify `--stdio` jail-spawn piping through a real Linux jail** — `spawn_plan` now pipes
  stdin/stdout (instead of leaving stdout on its prior default) when `--stdio` is in the command,
  mirroring `tddy-sandbox-darwin::spawn_plan`. Compile-checked only (the crate is
  `#[cfg(target_os = "linux")]`-gated and the dev environment that made this change has no Linux
  box); needs a real-jail run in Linux CI to confirm the daemon's now-stdio-only session control
  channel (`docs/dev/1-WIP/finish-stdio-ipc-migration.md`) actually works cross-platform.

### tddy-sandbox-app (source: specialized-subagents changeset, 2026-07-02)

- ~~`--specialized-agent` CLI flag + deprecated aliases~~ — done (2026-07-02, multi-agent tool-replacement changeset; overrides and the deprecated alias fully removed 2026-07-02 in a follow-up cleanup — see below). `tddy-sandbox-app` takes repeatable `--specialized-agent <name>` + `--agents-dir`, resolves them via `spawn::resolve_specialized_agents`, and threads the resolved array into the jail as `TDDY_SUBAGENT`/`TDDY_SUBAGENTS_JSON` via `spawn::subagent_env_overlay`. There is no `--discovery-subagent` alias and no `--fastcontext-*`/`--subagent-replaces` override flags — all configuration comes exclusively from the resolved agent's YAML def. See `docs/ft/coder/managed-codebase-subagents.md` and `docs/ft/coder/specialized-subagents.md`.
- **`--agent` CLI validation for custom specialized-agent names (`tddy-coder`)** — `create_backend` recognizes any resolved specialized-agent name, but `packages/tddy-coder/src/run.rs`'s clap `value_parser` on `--agent` still hardcodes a fixed allowlist and rejects a custom name (e.g. `my-explorer`) before `create_backend` ever runs. Fixing it requires resolving `<tddyhome>/agents` before `--tddy-data-dir` itself is parsed from CLI args — an ordering problem — and has no dedicated test (only `create_backend` itself is tested directly, bypassing clap). See the `TODO` comment at the `Args.agent` field.

### tddy-core (source: stdio-transport-for-grpc-binaries changeset, 2026-07-01)

- **Migrate the toolcall listener to tddy-rpc/tddy-stdio** — `tddy-core/src/toolcall/listener.rs` is a third bespoke newline-delimited-JSON protocol (`submit`/`ask`/`approve`/`list-actions`/`build`) between `tddy-coder` and the Claude Code CLI subprocess it spawns, distinct from the sandbox tool-IPC and gRPC-over-UDS relay this changeset migrates. Same category of problem, same fix would apply, deferred to keep this changeset scoped.

### tddy-daemon (source: stdio-transport-for-grpc-binaries changeset, 2026-07-01)

- **Switch `tddy-daemon`'s real session lifecycle onto the stdio transport** — `connection_service.rs`'s spawn/dial orchestration and `sandbox_session.rs`'s `dial_and_bridge` still spawn `tddy-sandbox-runner` with `--grpc-uds`/`--grpc-listen-port` and dial the tonic `SandboxServiceClient`, for every real sandboxed session. All the primitives to switch this over are built and proven end-to-end through a real Seatbelt jail (`bridge_sandbox_stdio`, `StdioSandboxClient`, transport-agnostic `run_host_relay`) — what remains is purely wiring the daemon's own spawn/dial call sites in `connection_service.rs`, deferred because that file's orchestration is large (1300+ lines) and the switch changes live transport behavior for every real session. Once done, `--grpc-socket`/`--grpc-uds`/`--grpc-listen-port` and their port/path-allocation code (`pick_free_loopback_port`, the `ready_marker` polling handshake) can be deleted outright — no dual-path fallback, per this repo's convention.
- **Linux (`tddy-sandbox-cgroups`) jail-spawn stdio piping** — `tddy-sandbox-darwin::spawn_plan` was updated to pipe stdin/stdout (instead of redirecting stdout to an egress log) when `--stdio` is in the command; `tddy-sandbox-cgroups` needs the equivalent change on Linux. Not attempted in the original changeset because that crate is `#[cfg(target_os = "linux")]`-gated and couldn't even be compile-checked on the macOS dev environment that did the work, let alone verified through a real jail.

### tddy-sandbox-cgroups (source: sandbox-builder changeset, 2026-06-28)

- **Minimal RO-root `pivot_root`** — the sandbox-builder changeset lands read-only bind-mounts of each declared `ReadSpec` inside the rootless jail, but the jail still shares the host filesystem root. Build a minimal tmpfs root, bind only the plan's reads + writable project/scratch/egress, then `pivot_root` into it for full filesystem write-confinement.

### tddy-build (source: tddy-build-bazel-system changeset, 2026-06-16)

- **Distributed cache / parent-fallback** — remote shared cache layer (maker-build pattern). Deferred to v2.
- **Hermetic sandboxing** — isolate action execution; v1 uses PATH + cwd discipline only.
- **Full remote build execution** — `TDDY_SOCKET` relay covers co-located sessions; true remote/distributed build deferred.
- **Watch mode** — incremental rebuild on file change.
- **Output-publication convention** — finalize the published-artifact layout (maker-build publishes to `dist/{name}/`); v1 stages under `.tddy-build/out/{target_id}/` only.
- **Cross-compilation architecture filter** — port `ensure_action_architecture()` from `session_actions` for ToolTargets that ship per-arch binaries.
