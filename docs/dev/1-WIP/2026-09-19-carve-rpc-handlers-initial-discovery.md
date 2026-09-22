# Initial discovery — carve-rpc-handlers (`#carve` 11/11)

**Changeset:** [2026-09-19-carve-rpc-handlers.md](./2026-09-19-carve-rpc-handlers.md)
**Tree explored:** `feature/carve/rpc-handlers` rebased onto `feature/carve/pr-stack-crate` @ `a2bceddb`
(10/11's implementation pushed; `tddy-pr-stack` exists).

## Combined conclusions

1. **The drafted crate-move plan is not implementable.** Three of the four destination crates would
   close a dependency cycle, and **no open `#carve` node removes any of the three edges**:
   - `tddy-daemon-kernel → tddy-discovery` — `CatalogHandler`'s body needs `DaemonConfig` (kernel),
     `ModelRegistryStore` (`tddy-model-registry → tddy-discovery`) and `tddy-spawn` (`→ kernel`).
   - `tddy-daemon-sandbox → tddy-tool-engine` and `tddy-sandbox-runner → tddy-tool-engine` —
     `ExecToolHandler`'s body needs `WorkspaceSandboxRegistry`, `HostedAgentClones`
     (`tddy-session-agents → tddy-daemon-livekit → tddy-daemon-sandbox`), a LiveKit forwarder and
     `tddy_sandbox_runner::ExecuteToolResponse`.
   - `tddy-daemon-livekit → tddy-worktree-service → tddy-projects` — `ProjectHandler`'s bodies need
     two `tddy-daemon-livekit` forwarders.
2. **`PrStackHandler` is the one trait that can relocate.** It is defined in `tddy-session-lifecycle`
   (`pr_stack_rpc.rs:21`), not in a domain crate. `tddy-pr-stack` (10/11) depends on `tddy-github`,
   which depends on `tddy-service`, so the trait, `PrStackServiceImpl` and `build_pr_stack_entry` can
   move there acyclically. Its **body** cannot: it needs `session_list_enrichment`,
   `hooks_and_urls`, `session_reader` and other `pub(crate)` session-lifecycle code, and session code
   calls it on `self` (`svc_pr_status_for_caller.rs:222`, `:327`).
3. **`build_pr_stack_entry` names `tddy_workflow_recipes::PR_STACK_SERVICE`.** `tddy-pr-stack` cannot
   depend on `tddy-workflow-recipes` (the reverse edge exists), so the const moves with the entry
   builder and recipes re-exports it.
4. **Developer decisions (2026-09-22), final.** The target is about 10k production lines for
   `tddy-session-lifecycle` and `tddy-core`. `#carve` is extended. **#520 creates
   `tddy-daemon-rpc`, a crate above the lifecycle crate, and moves the four RPC families' bodies into
   it.** `PrStackHandler`, `PrStackServiceImpl` and `build_pr_stack_entry` move to `tddy-pr-stack`.
   `tddy-core` gets its own node. This supersedes an intermediate "split in place" decision; see
   Exploration 3.
5. **Kernel ports are not needed.** A crate above can depend on everything the bodies use. What is
   needed is **one port in the other direction**, `DaemonRpcFamilies`, for the two reverse edges:
   the peer-owned stack-base and named-node paths of session start, and a session room's served
   roster. The composition root fills it.
6. **The claimed shared behaviours are narrower than drafted.** `record_rpc_activity`: Catalog 1/4
   methods, ExecTool 3/4, PrStack 2/8, Project 0. `resolve_os_user`: PrStack only; the other three
   inline the same two-line check, and ExecTool has `authorize_exec_tool_caller`. The third, unnamed
   shared behaviour is **peer routing** (`rpc_served_by_peer`, `classify_addressed_daemon_route`,
   `common_room_slot` — `config`, `eligible_daemon_source`, `common_room_livekit_room`), used by
   PrStack and ExecTool, and by Project through the LiveKit forwarders.
7. **Transitive field sets** (the budget each handler struct is held to):

   | Handler | Fields | Which |
   |---|---|---|
   | Project | 6 | `config`, `user_resolver`, `tddy_data_dir`, `eligible_daemon_source`, `spawn_client`, `common_room_livekit_room` |
   | Catalog | 5 | `config`, `user_resolver`, `tddy_data_dir`, `model_registry`, `idle_tracker` |
   | ExecTool | 9 | `config`, `user_resolver`, `tddy_data_dir`, `eligible_daemon_source`, `common_room_livekit_room`, `idle_tracker`, `hosted_agent_clones`, `task_registry`, `workspace_sandboxes` |
   | PrStack | 7 | `config`, `user_resolver`, `tddy_data_dir`, `github_token_store`, `idle_tracker`, `eligible_daemon_source`, `common_room_livekit_room` |

8. **Helpers shared with session code** must end up callable from both without the handler holding
   the host: `run_exec_tool_locally`, `run_hosted_clone_tool`, `hosted_clone_for`,
   `resolve_exec_tool_worktree`, `exec_tool_route`, `resolvable_agent_defs`, `resolve_os_user`,
   `record_rpc_activity`, and the peer-routing trio.
9. **The drafted load-bearing guard is weak.** `local_socket_reachability_acceptance.rs` greps
   `local_socket_server.rs` for server type names; `LocalSocketServices` is generic, so no handler
   swap in `runtime.rs` can fail it, and it never names Project or Session.
   `tddy-daemon/tests/local_token_uds.rs` assembles its **own** services from `test_service`, so it
   cannot see `runtime.rs` either. **Nothing today dials the socket `runtime::build` assembles.**
10. **`DaemonSessionHost` still has 31 fields including `telegram`** on this tree — 8/11 (#494) had not
    greened its field replacement. The `telegram` field is not in any of the four sets above.

## Exploration 1 — verify the drafted premises (Explore agent, very thorough)

Scope: `DaemonSessionHost` fields and impls, the five handler traits, per-handler field/helper use,
`runtime.rs` construction, destination-crate dependencies, kernel ports, tests, shape-test style.

### Premise verdicts

| Claim | Verdict | Evidence |
|---|---|---|
| 31 fields | holds | `connection_service.rs:130-231`, `#[derive(Clone)]` |
| 29 impl blocks | fails | **41**: 10 trait + 31 inherent |
| 10 traits, 5 families | partly | 7 of 10 are RPC families; `StackParentHost`, `SessionTerminalBridge`, `RemoteSnapshotSource` are not |
| PrStack 5 / ExecTool 13 / Catalog 7 / Project 6 fields | 7 / 9 / 5 / 6 transitively | see §3 |
| `ProjectHandler` pure delegation | impl yes; bodies no | bodies in `project_coordinate_handlers.rs` (535 lines) |
| all four call `record_rpc_activity`; three call `resolve_os_user` | fails | Project never; `resolve_os_user` only PrStack |
| `*ServiceImpl<H>` generic | holds for all five | all built with `H = DaemonSessionHost` |
| no new edge / no cycle | fails for 3 of 4 | see Combined conclusions 1 |
| field types are foundation types | fails | 7 field types are session-lifecycle's own; `IdleTimeoutTracker` is `tddy-task`'s |
| `tddy-pr-stack` exists | failed at first look; **holds** after 10/11 pushed `a2bceddb` | |

### Fields of `DaemonSessionHost` (`connection_service.rs:130-231`)

| # | Field | Type origin |
|---|---|---|
| 1 | `config: DaemonConfig` | tddy-daemon-kernel |
| 2 | `sessions_base_for_user: SessionsBaseResolver` (`#[allow(dead_code)]`) | tddy-daemon-kernel |
| 3 | `tddy_data_dir: PathBuf` | std |
| 4 | `user_resolver: SessionUserResolver` | tddy-daemon-kernel |
| 5 | `spawn_client: Option<Arc<SpawnClient>>` | tddy-spawn |
| 6 | `eligible_daemon_source: Arc<dyn EligibleDaemonSource>` | tddy-host-service |
| 7 | `common_room_livekit_room: Option<Arc<RwLock<Option<Arc<Room>>>>>` | livekit |
| 8 | `telegram: Option<Arc<TelegramDaemonHooks>>` | session-lifecycle |
| 9 | `claude_cli_manager: Arc<CliSessionManager>` | session-lifecycle |
| 10 | `sandbox_manager` | tddy-daemon-sandbox |
| 11 | `workspace_sandboxes: Arc<WorkspaceSandboxRegistry>` | tddy-daemon-sandbox |
| 12 | `workspace_sandbox_provisioner` | tddy-daemon-sandbox |
| 13 | `task_registry: TaskRegistry` | tddy-task |
| 14 | `idle_tracker: Option<Arc<IdleTimeoutTracker>>` | tddy-task |
| 15 | `room_roster` | tddy-daemon-livekit |
| 16 | `roster_keepalive_interval` | std |
| 17 | `demo_vm_state` | session-lifecycle |
| 18 | `session_stdio` | session-lifecycle |
| 19 | `agent_activity_hub` | tddy-daemon-kernel |
| 20 | `session_agent_inference` | tddy-session-agents |
| 21 | `github_token_store: Option<Arc<dyn GitHubTokenStore>>` | tddy-github |
| 22 | `staging_base_dir` | std |
| 23 | `session_rooms` | tddy-daemon-livekit |
| 24 | `model_registry: Option<Arc<ModelRegistryStore>>` | tddy-model-registry |
| 25-27 | `session_agent_rosters`, `session_agent_clones`, `hosted_agent_clones` | tddy-session-agents |
| 28 | `session_admissions` | session-lifecycle |
| 29 | `agent_conversations` | tddy-session-agents |
| 30 | `session_notification_bus` | tddy-session-activity |
| 31 | `sandbox_rpc_bridge` | tddy-sandbox-runner |

### Trait impls on `DaemonSessionHost` (all under `src/connection_service/`)

| Trait | Impl file | Trait defined in |
|---|---|---|
| `SessionHandler` | `svc_session_lifecycle_ports.rs:17` | session-lifecycle `handler.rs:21` |
| `SessionService` | `svc_session_lifecycle_ports.rs:94` | tddy-service (generated) |
| `ProjectHandler` | `svc_project_ports.rs:16` | tddy-projects `handler.rs:13` |
| `ProjectService` | `svc_project_ports.rs:58` | tddy-service (generated) |
| `ExecToolHandler` | `svc_exec_tool_ports.rs:26` (326 lines) | tddy-tool-engine `exec_tool_service.rs:16` |
| `PrStackHandler` | `svc_pr_stack_ports.rs:27` (778 lines) | **session-lifecycle** `pr_stack_rpc.rs:21` |
| `CatalogHandler` | `svc_catalog_ports.rs:23` (197 lines) | tddy-discovery `catalog_service.rs:17` |
| `SessionTerminalBridge` | `terminal_bridge_impl.rs:8` | tddy-daemon-livekit |
| `RemoteSnapshotSource` | `agent_roster.rs:34` | tddy-daemon-livekit |
| `StackParentHost` | `stack_parent.rs:247` | session-lifecycle |

`ProjectService for DaemonSessionHost` only forwards to `ProjectHandler::x(self, ..)`.

### Per-handler reach (§3)

- **PrStack** — direct: `user_resolver`, `config`, `tddy_data_dir`. Via helpers: `pr_status_for_caller`
  (`config`, `github_token_store`), `base_sync_leg` (`config`), `record_rpc_activity`
  (`idle_tracker`), `rpc_served_by_peer` → `classify_addressed_daemon_route`
  (`config`, `eligible_daemon_source`) + `common_room_slot` (`common_room_livekit_room`),
  `resolve_os_user`. Free functions: `require_pr_stack_orchestrator` (`connection_service.rs:1505`),
  `owner_repo_from_repo_root`, `base_sync_*`, `pr_status_unavailable`, `pr_state_label`, `wire_same`,
  `session_list_enrichment::stack_plan_json_for_changeset`, `hooks_and_urls::validate_repoint_target`,
  `session_reader::DaemonSessionListing`. Crates: tddy-daemon-auth, tddy-worktree-service,
  tddy-workflow-recipes. **Reverse calls:** `svc_pr_status_for_caller.rs:222`
  (`resolve_stack_base`) and `:327` (`link_stack_node`).
- **ExecTool** — direct: `config`, `eligible_daemon_source`, `common_room_livekit_room`,
  `user_resolver`, `tddy_data_dir`. Via helpers: `record_rpc_activity` (`:31`, `:88`, `:236`),
  peer routing, `authorize_exec_tool_caller` (`svc_resolve_os_user.rs:127`), `hosted_clone_for`
  (`hosted_agent_clones`), `run_hosted_clone_tool` / `run_exec_tool_locally` (`task_registry`),
  `exec_tool_route` (`workspace_sandboxes`), `resolve_exec_tool_worktree` (`tddy_data_dir`). Shared
  with session code: `run_exec_tool_locally`, `run_hosted_clone_tool`, `hosted_clone_for`
  (`svc_start_hosted_agent_clone.rs`, `svc_session_agent_ports.rs`), `resolve_exec_tool_worktree`
  (session coordinate handlers, session files, hosted clone), `exec_tool_route`
  (`split_session.rs`, `svc_start_sandboxed_codebase_session.rs`).
- **Catalog** — direct: `config`, `model_registry`, `user_resolver`, `tddy_data_dir`. Via helpers:
  `record_rpc_activity` (`list_tools` only), `resolve_tddy_tools_path` (`config`),
  `resolvable_agent_defs` → `registry_agent_defs` (`tddy_data_dir`, `model_registry`; also used by
  session start). Free functions: `agent_models_cache`, `list_models_probe_args`,
  `parse_agent_models_json`, `AGENT_MODELS_CACHE_TTL` (`connection_service.rs:1760-1796`),
  `agent_allowlist_rows`, `agent_roster::subagent_info`, `resolve_cursor_binary_path`,
  `spawner::run_capture_as_user`.
- **Project** — `project_coordinate_handlers.rs`: `user_resolver`, `config`, `tddy_data_dir`,
  `eligible_daemon_source`, `spawn_client`, `common_room_livekit_room`. Calls no `self` helpers.
  Free functions: `merge_listed_projects_with_peers` (`connection_service.rs:1318`),
  `hooks_and_urls::{resolve_default_remote_or_empty, project_entry_from}`, `wire_same`,
  `service_util::*`, `project_path_under_home_from_user_relative`; LiveKit:
  `forward_add_project_to_host_via_livekit`, `forward_set_project_default_branch_via_livekit`.

Shared helper bodies:

```rust
// svc_resolve_os_user.rs:41
pub(crate) fn resolve_os_user(&self, session_token: &str) -> Result<String, Status> {
    let github_user = (self.user_resolver)(session_token)
        .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
    self.config.os_user_for_github(&github_user).map(|s| s.to_string())
        .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))
}
// svc_resolve_tddy_tools_path.rs:424
pub(crate) fn record_rpc_activity(&self) {
    if let Some(ref tracker) = self.idle_tracker { tracker.record_activity(); }
}
```

### Construction in `tddy-daemon/src/runtime.rs` (§4)

- One host: `DaemonSessionHost::new(...)` (`:950`) + `with_session_rooms`, `with_model_registry`,
  `with_session_notification_bus`, `with_idle_tracker`, `with_github_token_store` → `connection_arc`.
- `LocalSocketServices { .. }` (`:1038-1098`) wraps `connection_arc.{session_lifecycle_service,
  project_service, catalog_rpc_service, exec_tool_rpc_service, pr_stack_rpc_service}()`.
- `BinaryLocalSocketServices` (`:277-300`) hard-codes `<DaemonSessionHost>` for all five.
- Transport entries (`:1132-1144`): `catalog_entry`, `exec_tool_entry`, `pr_stack_entry`,
  `session_lifecycle_entry`, `project_entry`.
- The helpers: `svc_family_entries.rs:10-40`, `svc_project_ports.rs:95`,
  `svc_session_lifecycle_ports.rs:154` — each `XServiceImpl::new(Arc::clone(self))`.
- Also built in `test_util.rs` (`TestDaemon` forwards `CatalogService`, `ExecToolService`,
  `PrStackService`, `ProjectService` to `self.inner.*_rpc_service()`, `:244-373`) and in
  `tddy-daemon/tests/{local_token_uds,staging_forwarding_acceptance}.rs`.

### Destination-crate dependencies (§5)

- tool-engine: deps rpc, core, task, service, worktree-service; `tddy-session-lifecycle` dev-only.
- discovery: deps rpc, core, service, session-tool-client; kernel depends on it normally.
- projects: deps core, rpc, service; depended on by daemon, session-lifecycle, worktree-service.
- kernel: deps core, discovery, sandbox, livekit, rpc, service; no `tddy-task` edge.

### Kernel port pattern (§6)

`tddy-daemon-kernel/src/presenter_observer.rs`: `PresenterObserverSpawner`, `NoPresenterObserver`,
`SharedPresenterObserver`. Referenced only by that file and `telegram_extraction_shape.rs`.

### Tests per handler (§7)

- PrStack: session-lifecycle `tests/{link_stack_node_rpc,planned_pr_mutation_rpc,query_branch_resolution,
  cross_host_stack_parent,effective_spawn_branch,orchestrator_repo_root_resolution}_acceptance.rs`;
  `src/connection_service/{add_planned_pr_unit_tests,stack_child_link_tests}.rs`.
- ExecTool: `tests/{execute_tool,stream_execute_tool,relay_peer_forwarding}_acceptance.rs`, the
  workspace-tool-sandbox / seatbelt / sandboxed-codebase suites; tddy-daemon `relay_e2e`,
  `remote_managed_worktree_cross_host`, `session_agent_remote`, `session_room_cross_host`.
- Catalog: `tests/{list_agents_allowlist,list_subagents,registry_assistant_as_agent}.rs` + roster
  suites; `src/.../list_agent_models_{parse,probe}_tests.rs`; tddy-daemon
  `relay_idle_wired_acceptance.rs` (idle tracker via `ListTools`).
- Project: `tests/{add_project_to_host,set_project_default_branch,list_projects_local_only,
  list_projects_multi_daemon_aggregation,non_blocking_peer_fanout,supervisor_spawn_delegation}.rs`.
- `local_socket_reachability_acceptance.rs` (78 lines): text search of `local_socket_server.rs`
  for six server type names + absence of six hand-written adapter files.

### Shape-test style of earlier nodes (§8)

`env!("CARGO_MANIFEST_DIR")/..` paths, `read_to_string` + `contains`, a `production()` cut at the
first `#[cfg(test)]`, AC-labelled asserts with Given/When/Then comments and reasoned failure
messages. `telegram_extraction_shape.rs` adds one compile-level test importing the port.

## Exploration 2 — factor in the open `#carve` nodes (asked by the developer)

Question: does anything the open nodes will deliver remove a blocker from Exploration 1?

Read `## Responsibility` / `## Boundaries` of `2026-09-15-carve-telegram.md` (8/11, #494),
`2026-09-15-carve-presenter-split.md` (9/11, #495), `2026-09-15-carve-pr-stack-crate.md`
(10/11, #496), and node 10's in-progress worktree.

| Blocker | Removed by a node? |
|---|---|
| kernel → discovery | no — 8/11 only adds a kernel port; 9/11 is tddy-core only; 10/11 is recipes → pr-stack |
| daemon-sandbox / sandbox-runner → tool-engine | no — no node touches either crate |
| daemon-livekit → worktree-service → projects | no |
| `tddy-pr-stack` missing | **yes** — pushed in `a2bceddb` |
| PrStack body needs session-lifecycle internals + reverse calls | no |
| `telegram` field | 8/11 removes it; not in any handler's set |

Dependency checks run by hand:

```
tddy-daemon-kernel    -> core discovery sandbox livekit rpc service
tddy-daemon-sandbox   -> actions core daemon-kernel rpc sandbox sandbox-qemu sandbox-recipes sandbox-runner service stdio task tool-engine
tddy-daemon-livekit   -> core daemon-kernel daemon-sandbox host-service livekit rpc service workflow worktree-service
tddy-worktree-service -> core daemon-kernel rpc task service projects
tddy-model-registry   -> acp coder daemon-kernel discovery rpc service task tool-engine
tddy-pr-stack         -> core git github workflow           (a2bceddb)
tddy-github           -> … tddy-service                      (so pr-stack reaches the proto types)
tddy-service          -> core workflow rpc tui               (tui -> core; no path back to pr-stack)
```

`tddy-workflow-recipes` appears only as a **dev**-dependency of `tddy-service` and `tddy-livekit`.

`PrStackHandler` / `pr_stack_rpc::` consumers outside the two defining files:
`tddy-daemon/src/runtime.rs:297`, `tddy-session-lifecycle/src/lib.rs:142` (re-export),
`svc_family_entries.rs:6`, `svc_pr_status_for_caller.rs:1,222,327`,
`add_planned_pr_unit_tests.rs:10`.

`runtime::build` + `RuntimeOptions::for_binary()` is driven by `embedded_runtime.rs` and
`service_registration_acceptance.rs` (roster only, no socket). `DaemonConfig::local_socket_path()`
(`config.rs:1158`) honours `local.socket_path`, so a test can place the socket in a temp dir.

## Exploration 3 — widened scope: bodies move to a crate above (developer, 2026-09-22)

Developer decisions this pass rests on: target of about 10k production lines for
`tddy-session-lifecycle` and `tddy-core`; extend `#carve`; one crate above the lifecycle crate
(`tddy-daemon-rpc`); **#520 is widened** to create it and move the four RPC families; `tddy-core`
gets its own node.

### Production-line measurement

Method: lines before the first `#[cfg(test)]` that is followed by `mod`. Files named `*_tests.rs`,
`tests.rs` or `test_util.rs` count as tests.

| Crate | Production | In-`src/` tests |
|---|---:|---:|
| tddy-session-lifecycle | 22,067 | 9,597 (of which `connection_service` 15,562 production / 5,632 tests) |
| tddy-core | 23,418 | 7,893 |

`connection_service/` production lines by file (largest first): `svc_start_session_core` 908,
`session_coordinate_handlers` 818, `connection_service.rs` 808, `svc_pr_stack_ports` 778,
`svc_start_sandboxed_claude_cli_session` 661, `svc_session_agent_ports` 644, `svc_resolve_os_user`
538, `project_coordinate_handlers` 535, `svc_spawn_split_agent` 520,
`svc_start_sandboxed_cursor_cli_session` 505, `svc_session_files_ports` 499,
`svc_pr_status_for_caller` 483, `svc_provision_agent_clone` 471, `svc_resolve_tddy_tools_path` 458,
`svc_split_context_from_codebase_host` 457, `svc_ensure_session_room_for_agents` 436,
`svc_resolve_listed_worktree` 430, `svc_start_hosted_agent_clone` 424,
`svc_materialize_staged_attachment` 416, `svc_activity_ports` 388, `svc_exec_tool_ports` 326,
`agent_roster` 326, `hooks_and_urls` 311, `svc_relaunch_sandboxed_runner` 303, and 25 more files
under 300 lines each.

Files this node moves (total lines): `pr_stack_rpc.rs` 148, `svc_project_ports.rs` 105,
`project_coordinate_handlers.rs` 535, `svc_catalog_ports.rs` 197, `svc_exec_tool_ports.rs` 326,
`svc_pr_stack_ports.rs` 778, `svc_family_entries.rs` 40 — **2,129 lines**, plus the PR-stack half of
`svc_pr_status_for_caller.rs`.

### Reverse edges: session code that uses the families

- `svc_pr_status_for_caller.rs:222` — `PrStackHandler::resolve_stack_base(self, …)` in
  `resolve_chain_base_ref_status`, on the `StackParentRoute::OwnedByPeer` arm only. The `Local` and
  `NoParent` arms stay in-crate.
- `svc_pr_status_for_caller.rs:327` — `PrStackHandler::link_stack_node(self, …)` in
  `record_spawn_on_stack_node`, only when the caller named a node. The unnamed path calls
  `link_stack_node_to_spawned_branch`, which is in-crate (and unit-tested in
  `stack_child_link_tests.rs`).
- `svc_session_files_ports.rs:90-103` — `session_room_roster()` bundles `catalog_entry`,
  `exec_tool_entry`, `pr_stack_entry` and `project_entry` with six in-crate entries. Called from
  `svc_spawn_split_agent.rs:159` and `svc_resolve_listed_worktree.rs:425`.

### Callers of the host's family helpers

`*_service()`, `*_rpc_service()` and `*_entry()` are called from `tddy-daemon/src/runtime.rs`,
`tddy-daemon/tests/local_token_uds.rs`, `tddy-daemon/tests/staging_forwarding_acceptance.rs:201`,
`tddy-session-lifecycle/src/test_util.rs` (`TestDaemon` impls at `:244`, `:291`, `:328`, `:373`),
`svc_session_files_ports.rs`, and `tddy-session-lifecycle/tests/session_sync_livekit_acceptance.rs:217`.

### Fixtures available to a crate above

`tddy_session_lifecycle::test_util` is `pub mod` (`lib.rs:148`) and not `cfg(test)`, so a crate above
can use `test_service(sessions_base) -> TestDaemon`, `TestDaemon::{as_arc, connection}`,
`TEST_TOKEN` and `TEST_USER` (`testuser` → `testdev`). `ProjectServiceImpl::new`,
`CatalogServiceImpl::new` and `ExecToolServiceImpl::new` each take `Arc<H>`.

### Core grouping (for the successor node, answered to the developer)

Inbound and outbound `crate::` edges per module:
- `presenter` — used by 2 core files and 19 outside; uses 16 modules; `toolcall → presenter` is a back-edge.
- `backend` — used by 20 core files and 66 outside; `backend ↔ workflow` depend on each other.
- `workflow` — 18 core, 107 outside.
- `changeset` — 9 core, 112 outside; `workflow ↔ changeset` depend on each other.
- `session_actions` — uses only `atomic_file` and `output`.
- `log_backend` — uses nothing.
- `session_catalog` — uses `session_actions` and `toolcall`.
