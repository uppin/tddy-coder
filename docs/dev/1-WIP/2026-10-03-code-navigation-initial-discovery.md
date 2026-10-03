# Initial Discovery: Code navigation in the session code explorer

**Changeset**: [2026-10-03-code-navigation.md](./2026-10-03-code-navigation.md)
**Date**: 2026-10-03
**Passes**: 3 (the appended-nodes whole-work discovery, copied in full)

## Combined Conclusions

**Code intelligence to the web (code-navigation, plan-dialog).** The web code explorer is
`WorktreeCodePane` (`packages/tddy-web/src/components/session/WorktreeCodePane.tsx`): a lazy
`WorktreeFileTree` plus a read-only `CodeBlock` (PrismLight, no token/position hooks), fed by
`worktree.WorktreeService` (`ListWorktreeDirectory`, `ReadWorktreeFile`) served from
`packages/tddy-worktree-service/src/service.rs`, authorised by `resolve_listed_worktree`
(session token → OS user → project main repo → `worktree_path_is_listed`). `code_index.CodeIndexService`
has **no** definition/references/hover RPC; `tddy-lsp`'s `LspClient` already has typed
`definition`/`references`/`hover` helpers (`client/queries.rs:48–72`), and the index daemon reaches
an `Arc<LspClient>` per root through `WorkspaceIndex::client_for`. tddy-daemon's
`IndexDaemonRegistry` (`packages/tddy-daemon/src/index_daemon/registry.rs`) lazily spawns, readies,
reaps and shuts down the index daemon; `connect() -> tonic Channel` has **no production caller**
(todo `2026-09-16-indexdaemonregistry-connect-has-no-caller.md`), and the registry is built only
inside the `user_resolver` block of `runtime.rs` and only with an `index_daemon:` config section.
Nothing proxies `code_index` to the web; there are no `code_index` TS bindings
(`scripts/generated-code.manifest` generates tddy-web only from `tddy-service/proto` and
`tddy-terminal-rpc/proto`). Positions: `code_index` uses 1-based byte columns; LSP uses 0-based UTF-16.

**Session start (indexing-indicators).** Session start is one call (`StartSession` unary or
`StreamStartSession`); worktree creation (`service_util::create_session_worktree`, git fetch +
worktree add) and the optional semantic index (`index_session_worktree`, blocking) both run before
it returns; `StartSessionEvent` carries only `AttachmentMaterializationProgress` and the final
result. The web shows attachment progress rows and a disabled Create button — no phase text, no
indexing indicator; `SessionEntry` has no index/worktree readiness field. `Warm(workspace_root)`
streams `IndexProgress{line, phase, percentage, furthest, ready}` (furthest-seen, idempotent,
`ready:true` last).

**Sessions (session-code-tools).** LSP tools **already exist** in tddy-tools (`LspDefinition`,
`LspReferences`, `LspHover`, `LspSymbols`, `LspDiagnostics`, gated by `TDDY_LSP_TOOLS`), executed on
the host by `tddy_lsp_executor` via `tddy_tool_engine` — not via the index daemon. Restructure is
reachable only from the `restructure` CLI over `TDDY_INDEX_SOCKET`, which no session sets. A jailed
agent reaches the host only through allowlists (`IN_JAIL_RELAYABLE`, `IN_JAIL_RELAYABLE_EXEC_TOOLS`)
over `TDDY_SANDBOX_TOOL_IPC`; new session tools must be exec tools relayed to the host, which then
dials the index daemon. `server.rs` is over budget (new tools go in their own module); the advertised
tool set is pinned by `tests/mcp_tool_advertisement_audit.rs`; todo
`2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge.md` requires
the host to bind the session's worktree, never trust it from the jail.

**Restructure engine at `live-plans`' tip (transactional-groups, signature ops).** There is **no
per-op compile gate**: `refuse_a_broken_baseline` before writing and `refuse_a_broken_result` once
after the loop (`runner/compile_gate.rs`). **Nothing rolls back**: a failed run leaves edits on disk
by design (`AppliedTreeDoesNotCompile` message; pinned by
`apply_compile_gate_acceptance::leaves_the_edits_of_a_failed_apply_on_disk_for_inspection`); the
journal keeps only pre/post hashes; `OpStatus::Failed` is never written. `RefactorOp` lacks
`deny_unknown_fields` — a `"group"` field parses today and is dropped; `op.variant` is read by no
backend. `plan_store.rs` is still all `todo!()` at that ref, so group logic hooks into the two live
apply loops (`runner/entry_points.rs::apply`, `tddy-index-daemon/src/apply.rs::apply_plan`) and the
`check --deep` rehearsal (`runner/rehearsal.rs`). Rust `SUPPORTED` has 10 ops; caller-rewriting
assists go through `multi_file_assist` like `inline_method`; `references_at`/`references_outside`
and `rename_symbol`'s overlay+`minimal_edits` pattern are the base for engine-authored call-site
edits. Todo `2026-09-24-restructure-has-no-signature-operations.md` sketches `remove_unused_param`,
`change_return_type` (wrap/unwrap; callers left to the gate) and `change_signature`. **No plan op
repairs a call site today**, so `change_return_type`/`change_param_type` only compile at a group's
end if the group itself adapts the callers. `syn` 2 is already a workspace dependency
(`tddy-code-analysis`).

## Exploration 1: code-intel daemon/web path — 2026-10-03

**Agent**: Explore subagent · **Ref**: `origin/master` @ `ddd59902`

Sequence: confirmed ref; listed web files, `index_daemon` module, `code_index.proto`, `tddy-web/src/gen`;
read `WorktreeCodePane.tsx`, `worktreeFilesApi.ts`, `CodeBlock.tsx`, `WorktreeFileTree.tsx`,
`worktree.proto`, `code_index.proto`; grepped `"textDocument/` across packages; read
`tddy-lsp/src/client/queries.rs`, `tddy-index-daemon/src/{index.rs,warm.rs,service.rs,lib.rs}`,
`tddy-worktree-service/src/service.rs` (1–215, 500–600), `worktree_files.rs`,
`tddy-daemon/src/index_daemon{.rs,/registry.rs,/spawn.rs}`, `runtime.rs` (155–240, 370–390,
780–830, 1030–1135, 1277–1283), `tddy-lsp-executor/src/lib.rs`, `tddy-toolcall/src/toolcall/lsp.rs`,
`tddy-tool-engine/src/lib.rs:240–290`, `SessionMainPane.tsx`, `SessionsDrawerScreen.tsx`,
`rpc/selectedDaemon.tsx`, `buf.gen.yaml`, `scripts/generated-code.manifest`; session start:
`service_util.rs`, `claude_cli_spawn.rs`, `claude_cli_spawn_steps.rs`,
`session_coordinate_handlers.rs:329–400`, `svc_start_session_core.rs`, `session.proto`,
`CreateSessionPane.tsx:470–530`, `CreateSessionActions.tsx`, `StatusBar.tsx`,
`agentStatusDisplay.ts`, `useSessionAttachment.ts`, `tasks.proto`.

Key excerpts:

```rust
// tddy-worktree-service/src/service.rs:183–206 — the authorisation a navigation RPC must reuse
let os_user = self.authorize(session_token)?;
let main_repo = self.resolve_main_repo(&os_user, project_id.trim())?;
if !worktrees::worktree_path_is_listed(&main_repo, &worktree_path) {
    return Err(Status::failed_precondition("worktree_path is not a worktree of this project"));
}
// tddy-lsp/src/client/queries.rs
pub async fn definition(&self, uri: &str, pos: Position) -> Result<Vec<Location>, LspError>   // :48
pub async fn references(&self, uri: &str, pos: Position) -> Result<Vec<Location>, LspError>   // :56
pub async fn hover(&self, uri: &str, pos: Position) -> Result<Option<String>, LspError>       // :64
// tddy-daemon/src/index_daemon/registry.rs
pub async fn get_or_spawn(&self) -> Result<Arc<IndexDaemon>, IndexDaemonError>          // :74
pub async fn connect(&self) -> Result<tonic::transport::Channel, IndexDaemonError>      // :95, dials per call
```

```proto
// code_index.proto
rpc Warm(WarmRequest) returns (stream IndexProgress);   // :24
message IndexProgress { string line = 1; string phase = 2; uint32 percentage = 3; string furthest = 4; bool ready = 5; }
// session.proto:369–374
message StartSessionEvent { oneof event { AttachmentMaterializationProgress attachment_progress = 1; StartSessionResponse result = 2; } }
```

Findings: as in Combined Conclusions; web client pattern `useDaemonClientFor(WorktreeService,
selectedOwningHost)` (`SessionsDrawerScreen.tsx:482`); `WorktreeService` registered as
`ServiceEntry "worktree.WorktreeService"` at `runtime.rs:1277–1283`; `semantic_index` task visible
only in `TaskService.WatchTaskList`. Feature docs: `docs/ft/web/session-code-pane.md`,
`docs/ft/coder/warm-code-intelligence-daemon.md`, `docs/ft/coder/semantic-index.md`.

## Exploration 2: session tools path — 2026-10-03

**Agent**: Explore subagent · **Ref**: `origin/master`

Sequence: listed `tddy-tools/src`, `tddy-daemon-kernel/src`, `tddy-daemon/src/index_daemon`,
`docs/dev/todo`; read `index_client.rs`, `agent_tool_socket.rs` (kernel + daemon), `registry.rs`,
`spawn.rs`, `server.rs` (36–60, 290–360, 900–1005, 1505–1720, 2620–2700), `lib.rs`,
`action_tools.rs`, `mcp_primitives.rs`, `tool_list_announcer.rs`,
`tddy-session-tool-client/src/lib.rs` (transport detection), `tddy-lsp-executor/src/lsp_tools.rs`,
`tddy-tool-engine/src/lib.rs:225–310`, `jail_env_builders.rs`, `runner_env.rs`,
`tddy-sandbox-darwin/src/profile.rs:150–250`, `tddy-sandbox/src/spec.rs`,
`tddy-sandbox-runner/src/{runner.rs:85–120,host_relay.rs:195–240}`, `daemon_rpc_handler.rs`,
`tddy-service/src/session_agents.rs`, `host_agent.rs`, `run-index-daemon`.

```rust
// tddy-tools/src/server.rs:346–348 — how LSP tools are merged today
if tddy_lsp_executor::lsp_tools::lsp_tools_enabled() {
    tool_router.merge(dynamic_tool_router(&lsp_tool_defs()));
}
// tddy-sandbox-runner/src/runner.rs:95–109 — the in-jail allowlist
const FORWARDED_RPCS: &[(&str, &str)] = &tddy_service::session_agents::IN_JAIL_RELAYABLE;
// tddy-tools/src/index_client.rs:114–125 — the only code_index client
let channel = tddy_sandbox_runner::connect_uds_channel(socket).await...; Ok(CodeIndexServiceClient::new(channel))
```

Findings: three tool-registration patterns (static `#[tool]`, hand-built router like
`action_tools.rs`, catalog-driven `dynamic_tool_router` → `dispatch_session_tool` → host
`ExecuteTool`); transport order `TDDY_SANDBOX_TOOL_IPC` → LiveKit → daemon UDS → daemon HTTP;
Seatbelt allows AF_UNIX connects but only `spec.ipc_socket` gets explicit file grants — relay through
the host is the intended design; `index_client` is private to the binary and renders to the console
(an MCP tool needs structured output). Related todos: `2026-09-16-warm-ready-means-a-live-server-not-a-loaded-graph`,
`2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge`,
`2026-10-01-session-tool-client-repeats-its-transport-selection-three-times`,
`2026-08-23-the-action-tools-are-advertised-where-nothing-implements-them` (resolved; env-gated tools
precedent), `2026-10-02-*` anchors-path deadlines.

## Exploration 3: restructure engine at `live-plans` — 2026-10-03

**Agent**: Explore subagent · **Ref**: `origin/feature/live-plan/live-plans`

Sequence: read `runner.rs`, `runner/compile_gate.rs`, `runner/entry_points.rs`, `journal.rs:1–330`,
`runner/rehearsal.rs`, `overlay.rs:1–120`, `apply.rs:1–200`, `tddy-index-daemon/src/apply.rs`,
`plan.rs` (RefactorKind, RefactorOp, parse, parse_op), `plan_store.rs:1–420`, `backends/rust.rs`
(1–110, 240–340, 900–1420, 1590–1625, 1785–1960), `registry.rs:30–110`, `crate_move.rs` reference
types, `lib.rs:192–225`, skill `references/plan-schema.md`, todo
`2026-09-24-restructure-has-no-signature-operations.md`; grepped
`OpStatus::Failed|rollback|restore|git checkout`, `deny_unknown_fields|group`, `op.variant`,
`SUPPORTED|assist(|codeAction/resolve|textDocument/references`.

```rust
// runner/compile_gate.rs:75–76
/// The edits stay applied: the error says how to roll them back.
// runner.rs:55–87 commit_operation — hash pre, journal in_flight, apply, hash post, journal completed, ledger checkpoint
// backends/rust.rs:1791–1818 multi_file_assist — assist → codeAction/resolve → document_changes → convert_change
// backends/rust.rs:1598–1615 references_at — textDocument/references, includeDeclaration: false
```

Findings: as in Combined Conclusions; per-field refusal chain in `parse_op` (`plan.rs:693–806`) is
where `group` and new-op validation go; the journal is keyed by plan index (`usize`), not `OpId`;
`check --deep` skips a refused op without advancing the overlay; the skill's plan-schema op table
(`:73–92`) lists the 10 Rust ops.
