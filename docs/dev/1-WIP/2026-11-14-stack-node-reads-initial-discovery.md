# Initial discovery: stack-node-reads (`#orchestrators` 13/13)

**Base**: `origin/feature/orchestrators/sweep` @ `1ce80419a` (#901).

## Exploration 1: the two gaps, read against the sweep head (2026-11-14)

### How a session is classified for the stack tools today

- `tddy_tool_engine::stack_tools::stack_caller_role(&StackCallerFacts)`
  (`packages/tddy-tool-engine/src/stack_tools.rs:127`) is the pure decision. An orchestrator role
  (`is_orchestrator_role`: `project-home` or `orchestrator`) wins. Otherwise a `ParentStack` that
  binds the session gives `StackNode { stack_id }`.
- The facts are read by `stack_caller_of(sessions_base, session_id)`
  (`packages/tddy-daemon-rpc/src/exec_tool/stack_tools.rs:78`), with `changeset_of` (:116) and
  `node_binding` (:127). They read the session's `.session.yaml`, its changeset
  (`stack_id`, `orchestrator_session_id`), the orchestrator's changeset `stack_id`, and that stack's
  record in `ProjectStackStore::new(<base>/projects)`.
- **Every read is `tddy_core`** (`read_session_metadata`, `read_changeset`,
  `unified_session_dir_path`, `ProjectStackStore`). Nothing about it needs `tddy-daemon-rpc`. The
  TODO's premise ("the project stack store and the orchestrator's changeset … live behind
  `tddy-daemon-rpc`") is wrong about the records: they live in `tddy-core`/`tddy-changeset`. Only the
  function sits in `tddy-daemon-rpc`.
- `tddy-tool-engine` already depends on `tddy-core` (`stack_tools.rs` reads `StackRecord`, and
  `is_orchestrator_role` comes from `tddy_core::session_metadata`). `tddy-session-lifecycle`
  already depends on `tddy-tool-engine` (`svc_list_exec_tools_wiring.rs` imports
  `project_home_tools`). So moving the record reads into `tddy-tool-engine` puts the classification
  below both callers and inverts no dependency.

### Gap 1: the jail's `ListExecTools`

- Daemon side: `ExecToolRpcHandler::offered_catalog`
  (`packages/tddy-daemon-rpc/src/exec_tool/project_home.rs:204`) builds
  `session_offered_catalog_for(session_tool_role(&metadata), …)`. Then, for a session whose tool
  role is not `ProjectHome`, it appends `stack_tools_offered_to(role)` and
  `stack_git_tools_offered_to(role)` from `stack_caller_of`.
- Jail side: `list_exec_tools_from_jail`
  (`packages/tddy-session-lifecycle/src/connection_service/svc_list_exec_tools_wiring.rs:80`)
  answers `session_offered_catalog_for(...)` alone. That is the `TODO(stack-plan-tools)`.
- `BoundJailSession` (`daemon_rpc_handler.rs:17`) carries `session_dir`, documented as
  `<the session owner's sessions base>/sessions/<session_id>`. `sandbox_rpc_handler`
  (`svc_host_builders.rs:143`) builds it from the directory the daemon resolved for the jail. The
  sessions base is therefore two levels up. `unified_session_dir_path`
  (`packages/tddy-changeset/src/session_lifecycle.rs:45`) is `base.join("sessions").join(id)`.
- An `orchestrator`-role session has the `ProjectHome` tool role, so the project-home catalogue
  already carries the stack tools in a jail too. **Only a node session is missing them.** This was
  verified at 6e: the orchestrator-jail parity test passes on the contract.
- ⚠ `handle_rpc` (`daemon_rpc_handler.rs`) is an open code issue
  (`packages/tddy-session-lifecycle/docs/code-issues/complexity-daemon-rpc-handler-handle-rpc.md`,
  192 lines, nesting 8). The `ListExecTools` arm must not grow, so the change stays inside
  `list_exec_tools_from_jail`.

### Gap 2: a node's call forwarded to the host that holds its stack

- `ExecuteTool` (`packages/tddy-daemon-rpc/src/exec_tool/ports.rs:34`) forwards a request whose
  `daemon_instance_id` names a peer **before** authenticating. On the receiving host,
  `serve_project_home_for_caller_held_elsewhere`
  (`packages/tddy-daemon-rpc/src/exec_tool/project_home_remote.rs:55`) handles any stack tool for a
  session it does not hold. It calls `home_caller_confirmed_at_owner` (:154), which asks every peer's
  `ListSessions` with the caller's token and believes only the owner's own row. It then refuses any
  row whose `session_role` is not an orchestrator role.
- After that check, the stack-tool branch (:80) builds `StackCaller { role:
  Orchestrator { own_stack_id: None } }` and calls `serve_stack_tool_for_caller_held_elsewhere`
  (`stack_tools.rs:196`). That calls `execute_stack_tool`, which applies `addressed_stack_id`. A
  `StackNode` role defaults to its own stack and refuses any other stack by name.
- The receiving host holds the stack, so it can find a node bound to the caller
  (`StackNode.session_id == caller`) in its own store for the project the owner's row names. No proto
  change is needed. The TODO names this route as the alternative to widening `ListSessions`.
- `is_stack_tool_offered_to(StackNode, tool)` admits only `pr_stack_status` and `pr_read`, so a node
  is still refused every write. The git-tool branch (:96) keeps the orchestrator-only confirmation.

### Test harness

- `packages/tddy-daemon-rpc/tests/stack_plan_tools_acceptance.rs` has a two-host `World`
  (`InProcessPeers`, never LiveKit), plus `an_orchestrator_owning_a_stack_of`, `a_node_session_of`,
  `stack_tools_offered_to` (daemon `ListExecTools`) and `call_as`.
- The jail bridge is reachable without spawning a jail:
  `DaemonSessionHost::sandbox_rpc_handler(session_id, session_dir)` returns the production
  `Arc<dyn HostRpcHandler>`, and its `handle_rpc(EXEC_TOOL_SERVICE, "ListExecTools", payload)` is
  the call an in-jail `tddy-tools` relays. Calling it through `Arc<dyn …>` needs no trait import, so
  no dev-dependency is added.
- `tddy_changeset::stack_fixtures` (`a_stack_host`, `orchestrator`, `session`, `stack_id_of`) is
  available to `tddy-session-lifecycle` tests already. For `tddy-tool-engine` it needs a dev-dependency
  on `tddy-changeset` with `test-support`. That crate is internal and already in the tool-engine tree
  through `tddy-core`.

### Code-issue and TODO cross-check (Step 2b)

| Item | Verdict |
|---|---|
| `docs/dev/todo/2026-11-14-jail-list-exec-tools-misses-stack-node-reads.md` | ✅ resolved here |
| `docs/dev/todo/2026-10-26-forwarded-conversation-spawner-is-refused-the-start-tools.md`, its `#orchestrators` 7/11 stack section | ✅ resolved here (that section only) |
| the same file's main item (the conversation spawner's start tools) | — unrelated: still open |
| `packages/tddy-session-lifecycle/docs/code-issues/complexity-daemon-rpc-handler-handle-rpc.md` | ⚠ during: do not grow the `ListExecTools` arm |
| `packages/tddy-daemon-rpc/docs/code-issues/complexity-exec-tool-ports-list-exec-tools.md`, `complexity-exec-tool-ports-execute-tool.md` | ⚠ during: `ports.rs` is not edited |
| no claimed issue in `tddy-daemon-rpc`, `tddy-tool-engine` or `tddy-session-lifecycle` | — |
