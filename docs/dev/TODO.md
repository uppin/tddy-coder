# Development TODO

## Future Enhancements

### Terminal lazy scroll-up — LiveKit transport & unified surface (source: terminal-replay-viewport changeset, 2026-07-28)

- **LiveKit transport does not carry offset metadata.** `GhosttyTerminalGrpc` now owns the
  scroll-up progressive forward-fill via `GetTerminalHistory`, but `GhosttyTerminalLiveKit` still
  receives raw bytes only. Carry `end_offset`/`at_oldest` on the LiveKit terminal frames (or a side
  channel) so the LiveKit-backed terminal can use the same forward-fill flow.
- **Unified single-terminal surface.** The viewport integration uses two stacked ghostty-web
  terminals (live at `scrollback: 0`, older at `scrollback > 0`) to avoid resetting the live
  terminal (which would reintroduce the duplicate-pane bug). A unified single-terminal surface
  is infeasible today because ghostty-web has no "insert at top of scrollback" API and a live
  reset is unacceptable; revisit if a future ghostty-web release adds a prepend API that does
  not require a live-terminal reset.
- **Persisted scroll position across reconnects** is out of scope; the forward fill reveals the
  older terminal and appends from offset `0` toward the anchor.

### PR-Stack — status polling and stack hygiene (source: pr-stack-ux-recovery changeset, 2026-07-26)

- **Manual verification against the live stack was never done** — orchestrator session
  `019f9dd5-716d-7071-96ac-464ff7b98c2a` on `uppin/tddy-coder`: confirm node `attach-store` recovers
  (its recorded session is gone, so the row must offer Start session pre-filled to resume
  `feature/session-attach-docs/attach-store`) and that PR #351 shows on
  `feature/session-attach-docs/attach-proto`. It needs the daemon **rebuilt and installed**,
  `auth_storage` (`/var/lib/tddy/auth`) **created and chowned to the daemon user**, and a **re-login**
  (the widened `read:user repo` scope). Every automated suite is green; this is the one unverified path,
  and it is also the end-to-end check that the token store and the `remote` leg work against a real
  repo rather than a fixture.
- **The repoint recovery was never run against a live stack** — deferred to after merge by the
  developer (source: `pr-stack-repoint-dead-end`, 2026-07-26). Exercise a genuinely stranded node — a
  predecessor whose PR merged on GitHub and whose branch was deleted, with the plan still recording
  `open` — and confirm the row offers "Repoint to `<default>`", that taking it drops the dead parent and
  leaves the node startable, and that a refusal shows its reason. Do it on **two** projects: one that
  stores `main_branch_ref` and one that does not. The second matters most: sending an empty target used
  to select a different rule server-side and silently do nothing, and no automated test covers that path
  end to end.
- **GitHub poll volume is one lookup per rendered branch per 5s** — `useQueryBranch` polls every branch
  a row renders. Adding each node's *base* branch to the poll set costs nothing extra: a base is by
  definition some node's own `branch` and was already in the set (`resolvedBranches` is deduplicated).
  The rate-limit problem was entirely the **two hooks polling the same fact** — `usePrStatus` and
  `useQueryBranch` both reaching the same authenticated `GET /pulls` — which is fixed by removing
  `usePrStatus`. What remains is a fixed 5s interval per branch: a ten-node stack is ~2 calls/second,
  which is comfortable against a 5000/hour user limit but still linear in stack size and unaffected by
  nothing having changed. Batch the resolution into one call per stack, cache per branch with ETags, or
  back off when the response is unchanged.
- **Dangling `session_id` links are never scrubbed** — `DeleteSession` leaves the deleted session's id
  on the orchestrator's stack node; the orphan state is derived at render instead. A periodic (or
  delete-time) reconciliation would make the stored stack match reality, which matters for anything
  reading the changeset directly rather than through the web.
- **`origin/<branch>` freshness depends on the last fetch** — the start-blocked warning reads the
  local remote-tracking ref, so a branch pushed from another machine reads as missing until this host
  fetches. A periodic background fetch, or a fetch-on-demand from the row, would close the gap. Since
  `pr-stack-repoint-dead-end` this also has a **destructive** consequence, not just a delaying one: the
  row will offer "Repoint to `<default>`" for a base that is actually alive, and taking it drops the
  parent edge from the plan for good.
- **Two checked-in generations of `connection.proto` disagree** — `packages/tddy-rust-typescript-tests/gen/connection_pb.ts` predates `RepointPlannedPr` entirely, while
  `packages/tddy-web/src/gen/connection_pb.ts` is kept current. Regenerating the former is a large diff
  unrelated to any one changeset, which is why it keeps being skipped. Source: `pr-stack-repoint-dead-end`.
- **A repoint names its target but not what it drops** — the control reads "Repoint to `<target>`" and
  collapses the node onto that single parent, dropping every other edge (intended, D18). The operator is
  never shown *which* predecessors that removes, and there is no undo. Naming them in the button's
  tooltip, or a confirm step when more than one edge would go, would make the cost visible.
  Source: `pr-stack-repoint-dead-end`.
- **`repoint_planned_pr_node` has three pre-existing silent-failure paths** — a failed `git rev-parse`
  collapses to an empty `expected_sha`, which git reads as "the remote ref must not exist" and turns a
  `--force-with-lease` into a guaranteed rejection; `merge_base` failure invents `effective_base`; and a
  force-push failure is only `log::warn!`, so the RPC returns success while `origin/<branch>` still points
  at the old base and the PR was re-targeted anyway. Untouched by `pr-stack-repoint-dead-end`, which only
  moved them inside a branch guard. Source: `pr-stack-repoint-dead-end`.
- **A `tddy-coder`-embedded web server retains no GitHub token** — `packages/tddy-coder/src/run.rs`
  builds its own `AuthServiceImpl` without a `GitHubTokenStore`, so PR status there reads
  *unavailable* even after a real login. The daemon path is wired; this one is not.
- **`start_sandboxed_cursor_cli_session` takes no `stack_parent`** and calls neither
  `resolve_chain_base_ref` nor the node link, so a cursor-cli session cannot be a PR-stack child at
  all. Wiring it is a larger gap than this changeset, not an oversight in it.
- **Nothing tests that `qualified_head` is *applied* at the two call sites** — only that the function
  itself is correct. `get_open_pr` / `get_pr_by_head` reach `api.github.com` through free `curl`
  helpers with no injection seam, so pinning the outgoing `head` value would need either a network
  call or an HTTP-transport abstraction. Worth adding the seam if that module grows.

### tddy-web — Agent Activity tool-detail dialog (source: acp-tool-detail-explicit-states changeset, 2026-07-26)

- **No retry affordance on a failed body lookup** — `AgentActivityDetailDialog` reports a failure inline
  and nothing is cached, so a retry *is* possible but only by closing and reopening the row. A "Retry"
  button in the error block would make that discoverable.
- **The tool-detail cache has no per-session entry cap** — `AgentActivityRegistry`'s
  `MAX_SESSIONS = 100` LRU is the only bound, so a session in which the operator opens very many tool
  rows retains every fetched body for the page's lifetime. Bound it per session if a heavy transcript
  ever shows growth (related: the persisted-log size caps tracked below).
- **The `animate-pulse` skeleton is inline in the dialog** rather than a shared UI primitive — it is the
  only skeleton in tddy-web today. Promote it to `components/ui/` when a second surface needs one.

### tddy-sandbox-recipes — host-independent sandbox path tests (source: acp-tool-detail-explicit-states changeset, 2026-07-26)

- **`cursor_agent_prerequisite_reads` asserts a machine-specific path** —
  `packages/tddy-sandbox-recipes/src/cursor_cli.rs:509` asserts `/Users` appears among the path-traversal
  ancestor grants. That only holds where `HOME` sits under `/Users` (macOS); on Linux `HOME=/var/tddy`, the
  ancestors end at `/var`, and the test fails permanently. Introduced by c018a176 (#303) and already on
  `master`, so **no Linux developer can get a clean `cargo test --workspace`** — which trains everyone to
  ignore workspace failures.
  - **Tests should stub their environment** rather than read the ambient one: have
    `cursor_agent_prerequisite_reads` take the home/share roots (or resolve them through an injectable
    provider) so a test can pass a `tempfile::tempdir()` root and assert the *shape* of the ancestor chain —
    "every ancestor from the install dir up to the filesystem root is granted" — instead of a literal
    prefix belonging to one OS.
  - **Production code should be testable by design**: the same seam removes the hidden `HOME` dependency
    from the recipe, which is the actual reason the assertion had to name a real directory.
  - Audit sibling recipes for the same pattern before fixing just this one assertion.

### tddy-service / tddy-coder / tddy-build (source: bsp-build-server changeset, 2026-07-22)

- **Literal JSON-RPC 2.0 BSP transport** — the `bsp.BspService` is BSP-*shaped* over the workspace's
  protobuf/Connect + LiveKit RPC, so external BSP clients (Metals, IntelliJ-BSP) cannot connect. A real
  JSON-RPC 2.0 BSP server (`build/initialize`, `workspace/buildTargets`, …) over stdio/TCP with a `.bsp/`
  connection file would require a new transport.
- **Structured build diagnostics** — build ops return exit code + raw stdout/stderr only. Parse compiler
  output into `{file, line, severity, message}` diagnostics for `BuildTargetCompile`/`Test` responses.
- **Remaining BSP methods** — `buildTarget/inverseSources`, `dependencySources`, `dependencyModules`,
  `resources`, `debugSession/start`.
- **Unify shared `catalog` build_target rows with `build_targets`** — the populate task currently writes
  build targets into both the shared `catalog` table (lightweight, for the unified `list`) and the new
  `build_targets` table (rich, for BSP). Collapse the duplication via a SQL view once the read paths agree.
- **Streaming compile/test progress** — the initial compile/test/run methods are unary; add BSP-style task
  progress notifications (server-streaming) for long builds.
- **BSP per-request session construction (no cache)** — the daemon's `DaemonBspService` builds a fresh
  `BspServiceImpl` per request (each triggers a catalog open+populate). Fine for correctness; add a
  per-session cache (keyed by resolved `session_dir`, with eviction) if the read path gets hot.
- **Silent provider lowering failures** — `tddy-bsp`'s provider swallows a target's lowering error and lists
  it with empty sources/outputs (no `log` dependency on `tddy-bsp`). Add `log` and a `debug!` on failure if
  that observability is wanted.

### tddy-core / tddy-coder / tddy-daemon (source: session-catalog changeset, 2026-07-22)

- **`list_action_summaries` read-path cutover** — make the per-session catalog the sole read source: replace the query-time YAML glob in `packages/tddy-core/src/session_actions/list.rs` with a read from `SessionCatalog`. The producer (`PopulateCatalogTask`) and the consumers (`list-actions` listener / `tddy-tools` CLI / `tddy-sandbox-app` host) run in **different processes**, so this needs cross-process `catalog.db` reads via the durable `meta['populated_at']` marker + `CatalogError::PopulateTimeout` bounded wait, sync→async at the 3 call sites, and a lazy-populate-on-read model for the owner-less standalone CLI fallback.
- **Daemon populate trigger** — spawn `SessionCatalog::open_and_populate` in `tddy-daemon` `spawn_claude_cli_session_inner` on worktree-open (threads the shared `TaskRegistry` through the `self`-less free function; 3 call sites). The coder already triggers populate; the daemon-managed flow does not yet.
- **`SessionCatalog`/`CATALOG` lifecycle** — the process-global `DashMap` of per-session catalogs has no eviction (each holds a `SqlitePool`); add eviction on session-close, and add a bounded wait to `SessionCatalog::list` so a panicked populate task cannot hang a reader indefinitely.

### tddy-workflow-recipes / tddy-discovery (source: exploration-artifact changeset, 2026-07-21)

- **Prime `exploration.md` from the FastContext discovery subagent** — the discovery agent already returns `path:line-start-line-end` citations (`docs/ft/coder/discovery-agent.md`); seed the exploration artifact from those citations before the plan agent starts so plan-time exploration begins pre-warmed.
- **Structured exploration entries in `changeset.yaml` discovery** — extend `DiscoveryData.relevant_code` with line/col-aware references sourced from the exploration document, keeping a machine-readable mirror of the markdown.
- **Staleness detection for exploration line references** — flag `exploration.md` code references invalidated by later diffs (e.g. compare against `git diff` ranges in post-green steps) so downstream agents know which references to re-verify.

### tddy-github / tddy-daemon (source: cross-daemon-session-token changeset, 2026-07-04)

- **Refactor `TelegramOAuthStateSigner` to reuse the generic HMAC signer** — `packages/tddy-daemon/src/telegram_github_link.rs:48-135` hand-rolls the same HMAC-SHA256 sign/verify pattern that the new `SessionTokenSigner` (`packages/tddy-github/src/session_token.rs`) generalizes. Once the session-token signer lands, collapse the telegram state signer onto it.
- **Server-side session-token revocation / denylist** — signed tokens are only bounded by their 5-minute TTL; there is no way to revoke a leaked token before it expires. Add a shared (room-propagated) denylist only if leaked-token containment becomes a requirement.

### tddy-sandbox-app / tddy-sandbox-runner (source: claude-sandbox-launcher changeset, 2026-07-03)

- **Integration/acceptance test for `./claude-sandbox` full launch with an inline Ollama def** — the launcher was verified by a manual full-launch smoke test (config loads → `codebase_mode=managed` → inline `fastcontext` activated, end-to-end through a real macOS Seatbelt jail), but the interactive terminal-attach path was not exercised in CI and no automated regression test drives a full sandboxed launch with an inline Ollama `fastcontext` def. The launcher script, `tddy-sandbox-app --config`, the egress shim's plain-HTTP forward proxy, persisted `tddy-tools.mcp.log` + `latest` symlink, and the `--disallowedTools` + server-side replaced-tool enforcement are all landed; what's missing is a CI-runnable test that asserts the whole stack comes up and a subagent turn completes against a stubbed Ollama. Knowledge transferred to `docs/ft/coder/managed-codebase-subagents.md` § Standalone launcher; source changeset `docs/dev/1-WIP/claude-sandbox-launcher.md` removed after wrap.
- **Split the Standalone launcher section into its own `docs/ft/coder/claude-sandbox-launcher.md`** — the launcher/config/egress/observability/enforcement knowledge currently lives as a section of `managed-codebase-subagents.md`. If it outgrows that file, lift it into a dedicated feature doc.

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
