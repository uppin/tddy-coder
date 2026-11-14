# 2026-11-14 — Stack plan and read tools served by the daemon, addressed by stack id

**Type:** Feature

`#orchestrators` 7/11, PR [#884](https://github.com/uppin/tddy-coder/pull/884). Feature:
[PR stacking § PR-management tools](../../ft/coder/pr-stacking.md#pr-management-tools).

## What changed

- **`tddy-tool-engine`**: new `stack_tools` family — the eleven tool names, schemas
  (`stack_tools/defs.rs`), `is_stack_tool` / `is_stack_read_tool`, `StackCaller` /
  `StackCallerRole` classified by role alone (no recipe), `stack_caller_role`,
  `addressed_stack_id` (an orchestrator's own stack — `Changeset.stack_id` — is its default;
  a node session may address only its own stack), and `execute_stack_tool` over
  `StackToolPort`. `read_only::is_project_home_allowed_tool` admits the eleven.
- **`tddy-daemon-rpc`**: `exec_tool/stack_tools` serves the port over `ProjectStackStore` +
  `tddy-pr-stack` `stack_ops` / `pr_insight`, classifies the caller from the session record
  (`stack_caller_of`), routes `run_locally`, and forwards a call naming another
  `daemon_instance_id` to the holding host. GitHub is reached as the project's account.
- **`tddy-changeset`**: `StackRecord.title: Option<String>` (serde-default, so a record
  written without one still reads) and `ProjectStackStore::create_titled`;
  `pr_stack_create { title? }` stores it, `pr_stack_list` shows it.
- **`tddy-tools`**: the eleven `#[tool]` methods are gone; the tools are advertised from the
  daemon's `ListExecTools` and dispatched to it dynamically.
- Accepted gap: the jail `ListExecTools` bridge does not offer a stack node's two read
  tools (`TODO(stack-plan-tools)` in `svc_list_exec_tools_wiring.rs`; backlog
  `2026-11-14-jail-list-exec-tools-misses-stack-node-reads`). A forwarded call likewise
  confirms only an orchestrator-role caller, so a node session on another host cannot use
  its read tools there (noted in
  `2026-10-26-forwarded-conversation-spawner-is-refused-the-start-tools`).

## Measurements

- `tddy-tool-engine/src/stack_tools.rs`: created at 604 production lines, decomposed at
  wrap to 461 (`stack_tool_defs`, `object`, `stack_object` moved to `stack_tools/defs.rs`,
  161 lines) by `tddy-tools restructure`; no oversized-file record survives for it.
- `tddy-tools/src/server.rs`: 3,080 → 2,553 production lines — `oversized-file-server`
  record narrowed (partially fixed), not closed; the file is stack-shared with 10/11 and
  was not split.
- `tddy-tool-engine/src/lib.rs`: 891 → 892 (the `pub mod stack_tools;` line) —
  `oversized-file-lib` record updated; stack-shared, not split.
