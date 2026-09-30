# Initial Discovery: Going back in a subagent conversation takes its worktree back too (#agent-worktree 2/4)

**Changeset**: [2026-09-30-agent-worktree-rewind-reset.md](./2026-09-30-agent-worktree-rewind-reset.md)
**Date**: 2026-09-30
**Passes**: 2

## Combined Conclusions

1. **Git is CLI-only** across the workspace (no `git2`/`gix`). Reusable patterns: scratch-index
   snapshot (`tddy-daemon-livekit::session_room::write_wip_tree_within` + `publish_wip_ref`),
   binary diff (`session_room::diff_between`), `apply --check`/`apply` on stdin
   (`tddy-session-sync::Mirror::apply`), numstat parse
   (`tddy-worktree-service::worktrees::parse_git_diff_numstat`), worktree add/remove
   (`tddy-git::worktree`), test repo (`tddy-testing-commons::fs::temp_dir_with_git_repo`).
   None of them is a public "per-conversation worktree" primitive — a new crate is warranted.
2. **A subagent's mutating tools never run in-process.** `CodebaseAccess::Local` refuses every
   mutating and engine tool (`subagent.rs:444-580`); `CodebaseAccess::Managed` sends the call over
   `tddy-session-tool-client::dispatch_session_tool` as `ExecuteTool{session_id, token, tool_name,
   args}` to the facilitating daemon, which resolves the root from `.session.yaml` `repo_path` and
   runs on `HostWorktree` or in the workspace `Jail` (`local_exec_tools.rs:62-166`). ⇒ **All git
   operations are daemon-side.** The envelope carries no conversation id today.
3. **The jail's `ExecuteTool` relay drops everything but `tool_name`/`args_json`**
   (`tddy-sandbox-runner/src/runner.rs:79` → `relay.call_tool(&req.tool_name, &req.args_json)`),
   so a new `conversation_id` field must be carried through `SandboxSessionRelay::call_tool` too, or
   it is silently lost for every in-jail subagent.
4. **The workspace jail mounts only the session worktree** (`workspace_tool_sandbox.rs:226`), so
   the ephemeral worktree lives **inside** it: `<session worktree>/tmp/subagent-worktrees/<conv>`,
   excluded through `.git/info/exclude` (common dir) so the caller's `git status` stays clean. A
   linked worktree's `.git` file points into the common dir, which the jail may not see — another
   reason git runs host-side.
5. **The caller chooses the conversation id** (`server.rs:1707` — "the caller decides the
   conversation id; one is generated" otherwise). It becomes a path component and a branch name ⇒
   it must be validated (charset, no `/`, no `..`) before use.
6. **Read-only classification exists twice and disagrees**: `SubagentTool::is_mutating` = Write |
   StrReplace | Delete | Shell (`agent_def.rs:46`) vs the fail-closed
   `agent_tool_reads_the_clone` = only Read | Glob | Grep | SemanticSearch | ReadLints are read-only
   (`tddy-session-lifecycle/.../agent_roster.rs:154`, `pub(crate)`). The fail-closed one is right
   for commits (a background `Shell` job mutates during `Await`).
7. **Per-call hook points**: the daemon runs the tool then can `git add -A && git commit` in the
   conversation worktree and return the change facts inside `result_json`; the subagent loop's single
   append site `run_one_turn` (`subagent.rs:1505-1557`) turns that into a `ResultSummary` and a
   commit on the `TranscriptEntry`. `ResultSummary` (`subagent/result_summary.rs`) is externally
   tagged and bounded (final-frame budget, todo `…overflow-the-chunk-framing-threshold`).
8. **Rewind hook**: `take_turn` right after `transcript.rewind_to` (`subagent.rs:1893`); the cut is
   extended over trailing `tool` messages, so the reset target is the commit of the **last kept
   entry** (or the conversation base when no kept entry committed).
9. **Cancel path**: `server.rs:2065` `subagent_cancel_tool` → `retire` + `pending.cancel_conversation`
   (aborts the tokio task mid-turn, `subagent_runtime.rs:367-383`). A cancel can land between a
   tool write and its commit — harmless because the worktree is removed without a pull.
10. **The session-agent roster PRD explicitly rejected a writable clone for peer daemons**
    (`docs/ft/daemon/session-agent-roster.md` §Design decisions, "Reads local, writes proxied";
    Non-goals "Write-back from a remote clone", "Per-agent clone isolation on one host"). This stack
    is scoped to the in-process tddy-tools loop on the facilitating daemon and does **not** revisit
    the peer clone; those non-goals stand for peer-owned agents.
11. **Wire additions**: `ExecuteToolRequest.conversation_id = 6`; a new typed
    `ExecToolService/ConversationWorktree` RPC (op oneof, grown per node). `ExecToolHandler` is
    implemented once, by `tddy-daemon-rpc::exec_tool::ports::ExecToolRpcHandler`.
12. **Oversized files in the path**: `tddy-tools/src/server.rs` (≈3.7k), `tddy-discovery/src/subagent.rs`
    (≈2k), `subagent_runtime.rs`, `tddy-sandbox-runner/src/runner.rs`, `tddy-session-tool-client/src/lib.rs`
    — new logic goes into new modules.

## Exploration 1 — Explore agent: subagent tools, roots, summaries, rewind, git helpers, tests, docs

### 1. Sequence

A `grep -rn … --include=*.rs` failed because zsh expanded the glob, and one reply was cut off. Both were re-run with quoted patterns, so no step is missing.

1. `ls packages`, then `ls` of `tddy-discovery/src`, `tddy-discovery/tests`, `tddy-tool-engine/src`, `tddy-session-agents/src`, `tddy-tools/src`. Purpose: layout.
2. `ls src/subagent src/subagent_runtime src/roster`, plus line counts of the core files. Purpose: module sizes.
3. `git log --oneline -15`, `git status`. Purpose: recent subagent-control stack #545–#559.
4. Read `tddy-discovery/src/tools.rs` in full. Purpose: the READ/GLOB/GREP executor.
5. Read `tddy-discovery/src/lib.rs`.
6. Read `tddy-discovery/src/subagent.rs` 1–700 and 700–2001. Purpose: `CodebaseAccess`, dispatch, the turn loop, `take_turn`.
7. Read `agent_def.rs` in full. Purpose: `SubagentTool` and `is_mutating`.
8. Read `subagent/result_summary.rs` in full.
9. Read `subagent/transcript.rs` in full.
10. Read `subagent/turn_request.rs` in full.
11. Read `subagent/replacement.rs` in full.
12. Read `subagent/yield_condition.rs` 1–220.
13. Read `tddy-tool-engine/src/lib.rs` in full. Purpose: `execute_tool`, `contain_path`, StrReplace.
14. Read `edited_region.rs`, `exec_tool_service.rs`, `catalog.rs`.
15. Read `shell.rs` 1–200 and `contained_shell.rs`.
16. `grep -n "subagent_" packages/tddy-tools/src/server.rs`. Purpose: the MCP tool surface.
17. Read `server.rs` 1660–2120 (tool bodies), 2365–2694 (schemas and router), 2694–2753 (list and status).
18. `grep` for `subagent_config_from_env`, `open_remote_agent_session`, and related names. Found them in `mcp_primitives.rs`.
19. Read `tddy-tools/src/mcp_primitives.rs` and `lib.rs` in full.
20. `grep` of the `subagent_runtime.rs` API, then read lines 1–720.
21. `wc` of roster modules and `grep` of `conversation.rs`, then read `conversation.rs` 1–470.
22. `grep -rln ResumeAgentConversationRequest --include=*.proto`, then `grep` and read `packages/tddy-service/proto/session_agents.proto` 1–90.
23. `cat` of `agent_clone_worktree.rs`, `agent_clone_lookup.rs`, `clone_readiness.rs`, `hosted_clone_start.rs`, and the session-agents `lib.rs`.
24. Outlined and read `session_agent_clone.rs` 1–80, 80–500, 815–1255.
25. `cat exec_tool_caller.rs`, plus a `grep` of `service.rs`, `agent_conversations.rs`, `ports.rs` for access wiring.
26. Read `service.rs` 150–450 (`take_a_turn`, `TurnRequest` mapping) and 775–955 (open, prompt, resume, cancel).
27. `grep` for `fn open_local` / `fn open_owned` and every `CodebaseAccess::` construction.
28. Read `tddy-session-lifecycle/src/connection_service/svc_start_hosted_agent_clone.rs` 140–340.
29. `grep` for `resolve_exec_tool_worktree`, `run_exec_tool_locally`, `run_hosted_clone_tool`, `dispatch_envelope`.
30. Read `connection_service/local_exec_tools.rs` in full and `svc_resolve_os_user.rs` 40–188.
31. `grep -B3 -A25 agent_tool_reads_the_clone` in `connection_service/agent_roster.rs`.
32. `ls` of the git crates (`tddy-git`, `tddy-session-worktree`, `tddy-worktree-service`, `tddy-session-sync`, `tddy-remote-git-repo`) and `grep git2|gix` over every `Cargo.toml`. No hits.
33. `grep` of the public API of `tddy-git`, `tddy-session-sync`, `tddy-session-worktree`.
34. Read `tddy-session-sync/src/apply.rs` and `mirror.rs` 85–332.
35. `grep` for `wip_ref`, `--binary`, `commit-tree`, `write-tree`, `GIT_INDEX_FILE`.
36. Read `tddy-daemon-livekit/src/session_room.rs` 165–365, the fn outline, 920–1040, 1110–1180.
37. `grep` for `parse_git_diff_numstat` and read `tddy-worktree-service/src/worktrees.rs` 777–801. Read `tddy-git/src/worktree.rs` 1–60 and 380–420.
38. `grep '"clone"'` across `.rs` files and outline `svc_provision_agent_clone.rs`.
39. `grep` and read `tddy-session-tool-client/src/lib.rs` 25–60, 227–300, 340–420.
40. `grep` and read `tddy-discovery/src/openai.rs` 210–349. Purpose: model-facing tool definitions.
41. `wc` of the discovery tests, `grep` of helpers in `tool_result_summary_acceptance.rs`, `subagent_write_tools_red.rs`, `resume_replacement_acceptance.rs`, then read `tool_result_summary_acceptance.rs` 1–130.
42. `grep` of `subagent_resume_red.rs` and `subagent_message_ids_red.rs` helpers, `ls tddy-testing-commons/src`, and `Cargo.toml` dev-dependencies.
43. `grep` of the `tddy-testing-commons` API and read `fs.rs` 15–60. Read `tddy-tools/tests/subagent_resume_mcp_acceptance.rs` 1–140 and `grep` its helpers and env vars.
44. `ls` of `tddy-session-agents/tests` and the agent-related `tddy-session-lifecycle/tests`. Read `agent_conversation_system_prompt.rs` 1–60 and `grep` its helpers.
45. Doc listings, `grep -i clone|commit|isolat` over the subagent PRDs (no hits), outline of `session-agent-roster.md`, read lines 519–638 and 1070–1122.
46. Read `managed-codebase-subagents.md` 171–335 and 550–572, `specialized-subagents.md` 266–275, and the recent `docs/dev/todo` listing.
47. `head -25` of six todo files, and `ls packages/tddy-discovery/docs/code-issues/`.
48. `grep` of the jail plan and read `tddy-daemon-sandbox/src/workspace_tool_sandbox.rs` 100–230. Read `workspace_session.rs` 255–300.

---

### 2. Inspected files and the excerpts that mattered

### `packages/tddy-discovery/src/tools.rs`
- The older executor for the discovery agent: READ/GLOB/GREP only, with `Local` and `Remote` modes (lines 23–33).
- Remote mode POSTs to `{daemon_url}/exec_tools.ExecToolService/ExecuteTool` (139–175).
- **Specialized subagents do not use it.** They go through `CodebaseAccess` in `subagent.rs`.

### `packages/tddy-discovery/src/subagent.rs` (2001 lines, flagged oversized)

**`CodebaseAccess`** (211–218):
```rust
pub enum CodebaseAccess {
    Local,
    Managed(ManagedDispatchFn),
}
```
- `ManagedDispatchFn` (207–209) is `Arc<dyn Fn(String, Value) -> Pin<Box<dyn Future<Output=String>>>>`.
- The tool name passed in uses catalog casing: `"Read"`, `"Write"`, `"StrReplace"`, …

**Local mutation is refused** (444–452):
```rust
fn reject_local_mutation(tool: &str) -> SubagentError {
    SubagentError(format!(
        "{tool}: write tools require managed codebase access (local subagents are read-only)"))
}
```
- `write`, `str_replace`, `delete`, `shell` are Managed-only (455–521).
- `await_job`, `read_lints`, `semantic_search` are Managed-only through `reject_local_engine_tool` (526–580).

**Model-name to catalog-name mapping** in `dispatch_tool_call` (999–1079):

| Model name | Catalog name |
|---|---|
| `READ` | `Read` |
| `GLOB` | `Glob` |
| `GREP` | `Grep` |
| `WRITE` | `Write` |
| `STR_REPLACE` | `StrReplace` |
| `DELETE` | `Delete` |
| `SHELL` | `Shell` |
| `AWAIT` | `Await` |
| `READ_LINTS` | `ReadLints` |
| `SEMANTIC_SEARCH` | `SemanticSearch` |

- Arguments are checked first with `validate_tool_arguments` (1007).

**Other pieces:**
- `CANONICAL_EXEC_TOOL_NAMES` (711–722): the same ten names, used for the `replaces` lists.
- `ToolDispatch` (850–873) has four outcomes: `Ran(Value)`, `NeverRan`, `Rejected{..}`, `Repeated`. `summary()` (917–924) calls `summarize(tool, value)` only for `Ran`.
- `SpecializedSubagentSession` fields (1350–1374): `client`, `model`, `max_turns`, `access: CodebaseAccess`, `transcript: Transcript`, `tools`, `cumulative`, `context_tokens`, `admission`, `repeated_calls`.
- `tool_definitions()` (1430–1437) advertises only the tools the def binds. `dispatch_bounded` (1447–1471) checks the binding and the repeat ledger, then calls `dispatch_tool_call(&self.access, …)`.

**Where each tool result is appended** (`run_one_turn` 1505–1557). This is the natural place to add a per-call commit:
```rust
let dispatch = self.dispatch_bounded(tool_call).await;
...
let result_summary = dispatch.summary(&tool);
...
let message_id = self.transcript.push_tool_result(
    ChatMessage::tool_result(dispatch.tool_result_payload(), tool_call.id.clone(), tool.clone()),
    produced_nothing, result_summary);
if let Some(condition) = fired { ... outcome.yielded_message_id = Some(message_id); return ... }
```

**`take_turn` order** (1872–1923): validate conditions and replacement, work out the budget, clear the repeat ledger, then:
```rust
if let Some(rewind_point) = request.rewind_point() {
    self.transcript.rewind_to(rewind_point).map_err(|e| SubagentError(e.to_string()))?;
}
let appended_from = self.transcript.len();
// push prompt, correction, append_replacement
let mut outcome = self.run_turn_loop(budget.turns, request.yield_conditions()).await?;
outcome.messages = self.transcript.descriptors_from(appended_from);
```

- `PromptOutcome` (101–124): `stop_reason`, `content`, `usage`, `messages: Vec<MessageDescriptor>`, `clamped_max_turns`, `fired_condition`, `yielded_message_id`.
- `SubagentConfig` (762–786): `access`, `system_prompt`, `provider_queue`. `SubagentRegistry::create` (1970–2000) calls `SpecializedSubagentSession::new(…, config.access, …)`.

### `packages/tddy-discovery/src/agent_def.rs`
**An existing mutating classification** (44–54):
```rust
pub fn is_mutating(self) -> bool {
    matches!(self, SubagentTool::Write | SubagentTool::StrReplace | SubagentTool::Delete | SubagentTool::Shell)
}
```
- `catalog_name` and `from_catalog_name` (58–89).
- The default tool list is `[Read, Glob, Grep]` (92–94).
- `SpecializedAgentDef` (107–150) uses `#[serde(deny_unknown_fields)]` and has `tools`, `max_turns`, `replaces`, `api_key`, `usage_notes`.

### `packages/tddy-discovery/src/subagent/result_summary.rs`
- `ResultSummary` (23–78) is externally tagged, camelCase. Variants:
  - `Read{first_line, chars_read, total_lines, truncated}`
  - `Grep{..}`, `Glob{..}`
  - `StrReplace{replaced, matched_lines, bytes_written}`
  - `Write{bytes_written}`
  - `Delete{deleted}`
  - `Shell{exit_code, stdout_chars, job_id}`
  - `Await{exit_code, completed}`
  - `ReadLints{lint_count}`
  - `Error`
- `summarize(tool, result)` (91–137) keys on the model's tool name. `STR_REPLACE` reads `matchedOccurrences` from the engine result (112–118).
- There is no file, line, or commit field yet. `edited_region` and `edited_line` are **not** summarized.
- The design keeps every summary bounded (9–12) because descriptors ride the final LiveKit frame, which has a 48 KiB budget.

### `packages/tddy-discovery/src/subagent/transcript.rs`
- `MessageId(String)` is opaque. Ids are minted as `m{n}` from a counter that never goes backwards, even across a rewind (243–244):
  ```rust
  self.next_ordinal += 1; let id = MessageId(format!("m{}", self.next_ordinal));
  ```
- `TranscriptEntry` (194–201) holds `id`, `message`, `is_error`, `result_summary`. There is nowhere yet to store a commit sha per entry.
- `MessageDescriptor` (107–134) holds `id, role, tool, tool_calls:[{name, arguments}], is_error, preview (240 chars), result_summary?`.
- `rewind_to` (320–336) cuts after the id and extends the cut over any `tool` messages that follow:
  ```rust
  let at = self.entries.iter().position(|entry| &entry.id == id).ok_or_else(|| RewindError(id.clone()))?;
  let mut keep_through = at;
  while self.entries.get(keep_through + 1).is_some_and(|entry| entry.message.role == "tool") { keep_through += 1; }
  self.entries.truncate(keep_through + 1);
  ```
  The clone reset must target the commit of the **last kept entry**, not the named id.
- `append_replacement` (160–190) mints `call_replacement_{n}` and two ids. No dispatch runs.

### `packages/tddy-discovery/src/subagent/turn_request.rs`
- Fields (29–42): `prompt`, `from_message`, `correction`, `max_turns`, `yield_conditions`, `replacement`.
- Builders: `prompting`, `resuming`, `from_message`, `with_correction`, `within_turns`, `with_yield_conditions`, `with_replacement`.
- `budget_within` clamps to `1..=50` (147–166).
- There is no opt-out field for resetting the clone yet.

### `packages/tddy-discovery/src/subagent/replacement.rs`
- `Replacement{tool, arguments, result}` (23–35). The keep-original-and-append design is noted at 5–8.
- `validate_replacement` (60–97): known tool, schema check, at most 16 KiB, must be JSON.
- A replacement of a mutating call **never runs**, so it would produce no commit.

### `packages/tddy-discovery/src/subagent/yield_condition.rs`
- `YieldCondition{tool, when: Outcome{fact}|Argument{field, contains}}`.
- `KNOWN_TOOLS` lists nine names (175–185). `SEMANTIC_SEARCH` is excluded.
- Limits: 8 conditions, 256-character needle.

### `packages/tddy-discovery/src/openai.rs` (210–349)
- The tool definitions come in three groups:
  - `discovery_tool_definitions()`: READ/GLOB/GREP
  - `mutation_tool_definitions()`: WRITE/STR_REPLACE/DELETE
  - `engine_tool_definitions()`: SHELL/AWAIT/READ_LINTS/SEMANTIC_SEARCH
- SHELL's doc (273–277) says it is "a mutating tool, gated exactly like the three above".

### `packages/tddy-tool-engine/src/lib.rs`
- `contain_path(worktree_root, arg)` (85–139) canonicalizes paths, refuses `..`, and walks up to the nearest existing ancestor. Every file tool is confined to one `worktree_root`.
- `execute_tool_with_env(worktree_root, tool_name, args_json, registry, session_id, extra_env)` (226–265) dispatches:
  - `Shell` / `Await` / `Lsp*` / `ReadLints` on their own paths
  - everything else inline, then `register_sync_task`
- `tool_write` (351–379) returns `{"bytes_written": n}`. It has **no** edited region and no created-or-updated flag.
- `tool_str_replace` (381–438) returns:
  ```rust
  serde_json::json!({ "replaced": true, "matchedOccurrences": count, "bytes_written": new_content.len(),
                      "edited_region": region.text, "edited_line": region.line })
  ```
- `tool_delete` (440–455) returns `{"deleted": true}`.
- `tool_shell` (607–681): `block_until_ms == 0` starts a background `ShellTaskBody` (a job). Otherwise it blocks, 30 s by default, and returns `{stdout, stderr, exit_code}`.
- `tool_await` (683–757) returns `{stdout, exit_code, completed}`. A shell job that edits files finishes during `Await`, so an `Await` can mutate the tree.
- `dynamic_proxy::is_native_tool_denied_in_remote_mode` (866–868) denies `Write|Edit|NotebookEdit`.

### `packages/tddy-tool-engine/src/edited_region.rs`
- This is #551, "an edit shows what it wrote": `EDITED_REGION_CONTEXT_LINES = 8`. `edited_region(content_after, edit_offset) -> {text, line}` (42–52).
- Only StrReplace uses it.

### `packages/tddy-tool-engine/src/exec_tool_service.rs` and `catalog.rs`
- `ExecToolHandler` exposes four RPCs: `execute_tool`, `stream_execute_tool`, `list_exec_tools`, `list_session_tool_calls`.
- `tool_catalog()` lists the ten `ToolDef`s.

### `packages/tddy-tool-engine/src/shell.rs` and `contained_shell.rs`
- `Shell` trait, `LocalShell`, `RemoteShell` (ssh BatchMode). `session_shell(root, ssh_host)` picks one (190–199).
- `run_contained` / `run_contained_argv`: stdin set to `/dev/null`, own process group, the group killed on timeout.

### `packages/tddy-tools/src/server.rs` (3743 lines)
- The router is built at 2595–2753. Tool bodies:
  - `subagent_new_session_tool` 1717
  - `subagent_prompt_tool` 1818
  - `subagent_resume_tool` 1860
  - `take_a_turn` 1955
  - `subagent_await_tool` 2041
  - `subagent_cancel_tool` 2065
  - `subagent_list_tool` 2112
  - `subagent_status_tool` 2260
- Schemas: `subagent_new_session_schema` 2375, `subagent_prompt_schema` 2441, `subagent_resume_schema` 2472, `yield_conditions_property` 2508, `max_turns_property` 2559, `subagent_await_schema` 2574.
- **Schema gap:** `subagent_resume_tool` parses `yieldConditions` (1896) and `replacement` (1900), but `subagent_resume_schema` (2472–2503) advertises neither.
- The local path builds `SubagentRegistry::from_defs(vec![def]).create(&agent_name, subagent_config_from_env())` (1780–1785). The remote path calls `open_remote_agent_session` (1753).
- **Cancel** (2065–2108):
  ```rust
  let cancelled = sessions.retire(session_id);
  sessions.pending.cancel_conversation(session_id, "conversation cancelled");
  crate::session_agents::session_agent_roster().close_conversation(session_id);
  write_accounting_file(&sessions);
  drop(sessions);
  cancel_remote_conversation(remote).await;
  ... serde_json::json!({ "cancelled": cancelled }).to_string()
  ```

### `packages/tddy-tools/src/mcp_primitives.rs`
- `subagent_codebase_access_from_env()` (38–50):
  1. `TDDY_SUBAGENT_CODEBASE_ACCESS` set to `local` or `managed` wins.
  2. Otherwise Managed if `detect_session_tool_transport()` finds a transport.
  3. Otherwise Local.
- Managed wraps `session_tool_client::dispatch_session_tool` (54–60).
- Also here: `subagent_route` (112–133), `open_remote_agent_session` (157–182), `cancel_remote_conversation` (221–234).

### `packages/tddy-discovery/src/subagent_runtime.rs`
- `SubagentConversation` (52–80): `agent`, `turns`, `model`, `provider`, `usage`, `context_tokens`, `session: Arc<Mutex<Box<dyn SubagentSession>>>`, `remote`.
- `subagent_sessions()` (157–160) is a **process-wide `OnceLock`**. It is documented as unsafe for multi-session hosts (147–156).
- `PendingTurns::cancel_conversation` (367–383) resolves running turns with an error and calls `turn.abort.abort()`. The tokio task is aborted mid-turn.
- `run_turn` (619–658): lock the session, `take_turn`, record accounting, `pending.resolve`. **This is where the "turn completion" hook could go** for pulling changes into the caller's tree.
- `prompt_outcome_json` (426–454) builds `{stopReason, content, usage:{inputTokens, outputTokens, totalTokens}, messages, clampedMaxTurns?, firedCondition?, yieldedMessageId?}`.
- `pending_turn_json` (478–492) builds `{responseId, pending:true, queuePosition, queueSize, provider, providerQueuePosition, providerQueueSize}`.

### `packages/tddy-discovery/src/roster/conversation.rs` (522 lines, over budget)
- `AgentConversationLink::take_turn` (125–188) reads chunks until `last`. Descriptors are rebuilt from the wire with `parse_message_descriptor` (438+), which leaves `arguments` empty. `result_summary_json` is parsed back.
- `turn_call` (213–250) maps to `PromptAgentConversationRequest` or `ResumeAgentConversationRequest{from_message_id, correction, max_turns, yield_conditions_json, replacement_json}`.
- `RemoteAgentSession` (358–408): `cumulative_usage` and `context_tokens` are always 0, `tail` is always empty.

### `packages/tddy-service/proto/session_agents.proto`
- Service methods (23–78): Attach, Detach, List, Stream, `OpenAgentConversation`, `PromptAgentConversation` (stream), `ResumeAgentConversation` (stream), `CancelAgentConversation`, `ReportAgentCloneState`, `ReportAgentConversationState`.
- `AgentCloneState` (80–88): UNSPECIFIED, LOCAL, PROVISIONING, READY, ERROR.
- `AgentConversationChunk` (90–118): `content_chunk=1, stop_reason=2, last=3, messages=4, clamped_max_turns=5, fired_condition_json=6, yielded_message_id=7`.
- `AgentMessageDescriptor` (126–144): `id, role, tool, tool_calls (names only), is_error, preview, result_summary_json`.
- `ResumeAgentConversationRequest` (282–306), fields 1–9, ending with `replacement_json=9`.
- Header (11–18): the jail relay allowlist names six methods. Any new RPC a jailed `tddy-tools` must reach has to be added there too.

### `packages/tddy-session-agents/src/lib.rs`
- `IN_JAIL_RELAYABLE` is pinned by a test to exactly six methods (`relays_exactly_the_six_operations_the_jail_is_allowed`). New pull or diff RPCs would change that test and `tddy-service::session_agents::IN_JAIL_RELAYABLE`.
- The test's doc notes that resume is "a destructive write".

### Peer-daemon clone modules in `packages/tddy-session-agents/src/`
- **`agent_clone_worktree.rs`** (17 lines). The whole file is `agent_clone_worktree_path(session_id, agent_id, clone: AgentClone) -> Result<PathBuf, Status>`. It returns `clone.worktree_path` or `failed_precondition`.
- **`agent_clone_lookup.rs`**: `agent_clone_for(...)` looks up the roster record, then `session_agent_clones.get(session_id, &record.daemon_instance_id)`. If the agent is served locally there is no clone.
- **`clone_readiness.rs`**: `refuse_unready_clone` allows only `Ready | Local`.
- **`hosted_clone_start.rs`**: `start_hosted_agent_clone(...)` builds a `CloneMirrorSpec` and spawns `run_clone_mirror`.

### `packages/tddy-session-agents/src/session_agent_clone.rs` (1255 lines)

**What the clone is for** (1–19): "the independent checkout a remote agent reads its files from, one per (session, owning daemon)"; "The mirror is one-way".

**Facilitating daemon side:**
- `AgentClone{codebase_session_id, state, error, worktree_path: Option<PathBuf>, divergences}` (85–97).
- `SessionAgentCloneStore` is keyed by `(session, daemon)`. Methods: `claim`, `get`, `record_report`, `fail`, `forget`, `for_session` (126–249). The comment says "for isolation a read-only mirror does not need" (123–124).

**Owning daemon side:**
- `HostedClone{session_id, facilitating_daemon_instance_id, codebase_session_id, worktree_path, client, session_token, _room}` (256–276).
- `execute_tool_on_facilitator` (284–357) sends `StreamExecuteTool` with `daemon_instance_id: ""`. Every mutation lands in the **facilitator's authoritative worktree**.
- `HostedAgentClones` is keyed by session (405–435).
- `CloneMirrorSpec` (438–475) and `run_clone_mirror` (483+).

**`CloneMirror` git operations, all via `tokio::process::Command::new("git")`:**
- `open` writes the `.tddy-session-sync.json` marker (845–888).
- `restore` (930–968):
  ```rust
  self.git(&["fetch", &fetch_source, &format!("+{wip}:{local}")]).await?;
  self.git(&["reset", "--hard", &format!("{local}^")]).await?;
  self.git(&["read-tree", "-u", "--reset", local]).await?;
  ```
- `note_local_changes` runs `git diff --name-only refs/tddy/wip --`. Any change inside the clone counts as a divergence and is undone by the next restore (983–1012).
- `on_activity` fetches the delta, then `mirror.apply` or reconcile (1015–1045). `on_worktree` restores (1056–1067).
- `git` / `git_output` (1129–1155) set `GIT_SSH_COMMAND` for the `tddy-remote-git-repo` shim.

### `packages/tddy-session-agents/src/exec_tool_caller.rs`
- `authorize_exec_tool_caller(config, user_resolver, req) -> os_user` must run before the hosted-clone branch.

### `packages/tddy-session-agents/src/service.rs`
- `take_a_turn` (162–285) routes to Local (spawns `session.lock().await.take_turn(requested)` under `select!` with `closed.notified()`) or to Remote (forwards to the peer).
- `TurnOnAConversation::turn_request` (347–373) is the wire-to-`TurnRequest` mapping. A new resume field such as a reset opt-out would be added here.
- `open_agent_conversation` (783–884) has three branches:
  - `open_owned`: this host owns the agent and reads its hosted clone
  - `open_local`: the facilitator's own agent on the session worktree
  - `peers.open`: remote
- `cancel_agent_conversation` (925–955) removes the conversation and calls `closed.notify_one()` on Local, then forwards on Remote.

### `packages/tddy-session-lifecycle/src/connection_service/svc_start_hosted_agent_clone.rs`
- `open_local_agent_session` (158–196) → `SpecializedSubagentSession::new(..., self.local_agent_codebase_access(session_id, session_dir, &record.agent_id, session_token), ...)`.
- `owned_agent_codebase_access(clone)` (247–269) → `service.run_hosted_clone_tool(&request, &clone)`.
- `local_agent_codebase_access` (273–331):
  ```rust
  let answer = match service.resolve_exec_tool_worktree(&request) {
      Ok((sessions_base, worktree_root)) => agent_roster::dispatch_envelope(
          service.run_exec_tool_locally(&request, &sessions_base, &worktree_root).await),
      Err(status) => serde_json::json!({ "is_error": true, "error": status.message() }).to_string()
  };
  ```
  **This is the daemon-side seam where a per-conversation clone root could replace `worktree_root`.**

### `packages/tddy-session-lifecycle/src/connection_service/local_exec_tools.rs`
- `exec_tool_route` (62–87): `HostWorktree`, `Jail`, or `Refused`, based on `.session.yaml` (`session_type == "workspace" && sandbox == true`).
- `run_exec_tool_locally(req, sessions_base, worktree_root)` (100–166):
  - HostWorktree → `tool_engine::session_shell(worktree_root, ssh_host)` then `execute_tool_on_shell`
  - Jail → `jail.execute_tool(req)`, with one rebuild retry
  - It then appends to `tool_call_log`.
- `run_hosted_clone_tool` (247–291) splits on `agent_tool_reads_the_clone`: reads run on the clone, everything else runs `execute_tool_on_facilitator`.

### `packages/tddy-session-lifecycle/src/connection_service/agent_roster.rs`
**A second read-only classifier, which fails closed** (151–159):
```rust
/// A name outside the catalog is **not** read-only. The split has to fail closed ...
pub(crate) fn agent_tool_reads_the_clone(tool_name: &str) -> bool {
    matches!(tool_name, "Read" | "Glob" | "Grep" | "SemanticSearch" | "ReadLints")
}
```
- `dispatch_envelope` (167–174).

### `packages/tddy-session-lifecycle/src/connection_service/svc_resolve_os_user.rs` and `workspace_session.rs`
- `resolve_exec_tool_worktree` (170–187): authorize, `sessions_base_for_user`, then `resolve_worktree_root_for_session`.
- `resolve_worktree_root_for_session` (`workspace_session.rs` 267–279) reads `.session.yaml` `repo_path`.

### `packages/tddy-daemon-sandbox/src/workspace_tool_sandbox.rs`
- The jail's `scratch_dir` holds `$HOME` and `$TMPDIR` (108). `mounts: vec![MountSpec::read_write(worktree_path)]` (226), described as "one mount, the session's checkout".
- **A tmp clone outside the worktree and scratch dir is not visible inside a sandboxed workspace jail.**

### `packages/tddy-session-tool-client/src/lib.rs`
- `SessionToolTransport` has `SandboxIpc`, `DaemonUds`, `DaemonHttp`, `LiveKit`, `IncompleteLiveKit` (33–…). `detect_session_tool_transport` (227–296) checks env vars in that order.
- `dispatch_session_tool(tool_name, args)` (347–420). The in-jail subagent's Managed dispatch sends `ExecuteTool` for the session. There is no conversation id in the envelope (`SessionToolEnvelope{session_id, session_token, daemon_instance_id}`).

### `packages/tddy-session-sync/src/apply.rs` and `mirror.rs`
- `Delta{seq, prev_seq, base_commit, patch (git diff --binary), scoped_paths}`.
- `ApplyOutcome{Applied, AlreadyApplied, NeedsReconcile(ReconcileReason{SequenceGap, BaseCommitMismatch, PatchRejected})}`.
- `Mirror::apply` (≈160–205) checks the sequence, checks HEAD, then:
  ```rust
  let checked = self.git(&["apply", "--check"], &delta.patch)?;
  ...
  let applied = self.git(&["apply"], &delta.patch)?;
  ```
  The patch is piped on stdin. This pattern suits pulling commits into the caller's tree.

### `packages/tddy-daemon-livekit/src/session_room.rs`
- `write_wip_tree_within(root, budget)` (177–236) stages a **scratch `GIT_INDEX_FILE`** (seeded by copying the agent's index), then `add -A` and `write-tree`. The agent's own index is never touched.
- `publish_wip_ref(root, session_id, head, tree)` (317–362) runs `commit-tree` with a fixed identity `tddy-daemon <tddy-daemon@tddy.invalid>`, then `update-ref`.
- `wip_ref_name` = `refs/tddy/session/{id}/wip`.
- `diff_between(root, from, to, paths)` (931–958) runs `git diff --binary --no-ext-diff --no-textconv`.
- `changed_paths_between` (960–980) runs `diff --name-only -z`.
- `numstat_within` runs `diff --numstat HEAD`, parsed with `parse_git_diff_numstat`.
- `git_output` (1144+) applies a deadline and sets `GIT_OPTIONAL_LOCKS=0` and a null stdin.

### `packages/tddy-worktree-service/src/worktrees.rs`
- `WorktreeNumstat` (744) and `parse_git_diff_numstat(stdout) -> {changed_files, lines_added, lines_removed, paths}` (777–801). This covers the lines-added/removed part of the new summary. Created vs updated vs deleted would need `--name-status`.

### `packages/tddy-git/src/worktree.rs`
- `create_worktree` (`git worktree add … -b`), `remove_worktree` (`worktree remove --force`, falling back to `rm -rf`), `list_worktrees`.
- No `clone`, `commit`, or `diff` helpers.

### Tests
- **`packages/tddy-discovery/tests/tool_result_summary_acceptance.rs`**, helpers:
  - `a_def(base_url)` (20)
  - `a_codebase_that_answers(n)` (40): a Managed closure with a fixed result
  - `a_turn_that_calls(tool, args)` (57), `a_final_answer(text)` (74)
  - `a_model_that_calls_then_answers` (83): wiremock `/v1/chat/completions`, `up_to_n_times(1)` then a fallback
  - `a_turn_over` (99), `the_tool_result(outcome)` (107)
- **`packages/tddy-tools/tests/subagent_resume_mcp_acceptance.rs`**, helpers:
  - `explorer_def_json`, `final_answer_response`, `a_turn_that_reads`
  - `spawn_mcp_server(_in)` (83/94): the real `CARGO_BIN_EXE_tddy-tools --mcp`, with an optional `current_dir` so Local READs work
  - `send_json_line`, `read_json_line`, `initialize_mcp_session`, `call_tool` (150), `tool_result_json` (170), `tools_list` (180)
  - env `TDDY_SUBAGENTS_JSON`
- **`packages/tddy-session-agents/tests/agent_conversation_system_prompt.rs`**:
  - Fake ports: `TheTurnLoopsThisHostOpened: AgentSessions`, `PortsAnOpenDoesNotUse` for the other ports
  - Builders `an_agent_this_daemon_runs`, `a_session_directory_with`, `an_open_carrying`, `a_daemon_serving`
- **`packages/tddy-testing-commons/src/fs.rs::temp_dir_with_git_repo(label)`** (20–56): `git init`, an initial commit on `master`, and `origin` pointing at itself.

### Docs
Covered in §4.8: `session-agent-roster.md` 519–638 and 1070–1122, `managed-codebase-subagents.md` 171–335 and 550–572, `specialized-subagents.md` 266–275, and six `docs/dev/todo` files.

---

### 3. Grep and glob index

| Pattern | Scope | Notable hits |
|---|---|---|
| `subagent_` | `tddy-tools/src/server.rs` | 1666, 1717, 1818, 1860, 1955, 2041, 2065, 2112, 2260, 2375, 2441, 2472, 2595 |
| `fn subagent_config_from_env\|fn open_remote_agent_session\|…` | `tddy-tools` | `mcp_primitives.rs:82,112,157,221` |
| `pub fn\|pub struct\|stopReason\|…` | `subagent_runtime.rs` | 40, 52, 157, 367, 426, 478, 583, 619 |
| `pub fn\|…from_message_id\|replacement` | `roster/conversation.rs`, `link.rs` | 125, 192, 213–250, 253, 365 |
| `ResumeAgentConversationRequest` in `*.proto` | repo | `packages/tddy-service/proto/session_agents.proto` |
| `rpc \|^message \|clone` | `session_agents.proto` | 32–77, 80–88, 90–118, 233–249, 282–306, 309–346 |
| `pub fn\|git\|fetch\|WIP` | `session_agent_clone.rs` | 85, 126, 256, 284, 405, 438, 483, 820, 930, 950–959, 1134 |
| `CodebaseAccess\|execute_tool_on_facilitator\|worktree_path\|take_turn` | `session-agents/src` | `service.rs` 38, 247, 347, 783, 893, 915, 925, 1007 |
| `fn open_local\|fn open_owned` | `packages` | `ports.rs:158,166`; `svc_session_agent_port_adapters.rs:158,173`; `svc_start_hosted_agent_clone.rs:158,204` |
| `CodebaseAccess::managed\|Local\|Managed` | `packages` | `mcp_primitives.rs:40–55`; `svc_start_hosted_agent_clone.rs:253,285`; many discovery tests |
| `fn resolve_exec_tool_worktree\|fn run_exec_tool_locally\|…` | `packages` | `svc_resolve_os_user.rs:54,170`; `local_exec_tools.rs:100,247`; `agent_roster.rs:167`; `svc_provision_agent_clone.rs:365` |
| `fn agent_tool_reads_the_clone` | `agent_roster.rs` | 154 |
| `git2\|gix` | `*/Cargo.toml` | **no hits** |
| `wip_ref\|--binary\|commit-tree\|write-tree\|GIT_INDEX_FILE` | `packages` | `session_room.rs:175,230,235,301,324,943`; `tddy-session-sync/src/sync.rs:55,136`; `tddy-pr-stack/src/git_ops.rs:230` |
| `parse_git_diff_numstat\|WorktreeNumstat` | `packages` | `tddy-worktree-service/src/worktrees.rs:744,777` |
| `"clone"` | `*.rs` | only tests, `tddy-projects/src/project_storage.rs` tests, `tddy-spawn/src/supervisor_spawn.rs:108` (process clone, unrelated) |
| `pub async fn dispatch_session_tool\|…` | `session-tool-client` | 33, 119, 227, 298, 347 |
| `name: "[A-Z_]*"` | `openai.rs` | 121–336 |
| test helpers `^fn\|^async fn` | discovery tests | listed in §2 Tests |
| `TDDY_[A-Z_]*` | `subagent_resume_mcp_acceptance.rs` | `TDDY_SUBAGENTS_JSON` (211, 267, 471); `roster/seed.rs:34` |
| `clone\|commit\|isolat\|git ` | the two coder PRDs | **no hits** |
| `read-only\|mutat\|write` | `session-agent-roster.md` | 30, 54, 525, 556–573, 953, 1076–1081, 1098 |
| `write-back\|subagent.*clone\|isolat` | `docs/dev/todo`, `TODO.md` | nothing specific to subagent clones. The subagent-relevant items are the recent todos in §4.8 |
| `worktree_root\|mount\|TMPDIR` | `workspace_tool_sandbox.rs` | 108, 152–155, 226 |

---

### 4. Findings

### 4.1 What a subagent can call, and read-only vs mutating (Q1)

**Tools and dispatch:**
- A def can bind any of the ten tools: `READ`, `GLOB`, `GREP`, `WRITE`, `STR_REPLACE`, `DELETE`, `SHELL`, `AWAIT`, `READ_LINTS`, `SEMANTIC_SEARCH` (`agent_def.rs:27–41`). There is no `Edit` or `MultiEdit`, and no LSP tools, although the engine supports LSP.
- Definitions live in `openai.rs:121/216/278`. Filtering happens in `subagent.rs:1430`. Dispatch happens in `subagent.rs:999–1079`, which calls `CodebaseAccess` methods (253–580).
- On the host side, `tddy_tool_engine::execute_tool_with_env` (`lib.rs:226`) runs the call.
- `tddy-discovery/src/tools.rs` is the older discovery executor and is not on the subagent path.

**Existing classifications. There are two, and they disagree:**
- `SubagentTool::is_mutating()` (`agent_def.rs:46`) = `Write | StrReplace | Delete | Shell`.
- `agent_tool_reads_the_clone()` (`agent_roster.rs:154`) treats `Read | Glob | Grep | SemanticSearch | ReadLints` as read-only and everything else as mutating, including **`Await`** and unknown names.
- The roster PRD table (`session-agent-roster.md:561–564`) agrees with the second one: `AWAIT` is proxied as a mutation.
- **Recommendation for the per-call commit rule:** use the fail-closed set, because a background `Shell` job can change files when its `Await` completes.
- **`Local` access refuses every mutating and engine tool** (`subagent.rs:448`). A Local-mode subagent is effectively read-only, so the clone work only matters on Managed paths.

### 4.2 Which root the tools act on (Q2)

**Local.** Raw `std::fs` against the process cwd, which in tests is `spawn_mcp_server_in`'s `current_dir`. There is no containment and no writes.

**Managed, in-jail `tddy-tools`.**
- `dispatch_session_tool` sends `ExecuteTool{session_id, token, tool_name, args}` to the daemon.
- The daemon resolves the root with `resolve_exec_tool_worktree`, which is `.session.yaml` `repo_path` (`workspace_session.rs:267`).
- `run_exec_tool_locally` then runs it on HostWorktree (`session_shell`, or ssh `RemoteShell`) or inside the Jail.
- **The envelope carries no conversation id**, so every conversation shares one root.

**Managed, daemon-run local agent.** `local_agent_codebase_access` (`svc_start_hosted_agent_clone.rs:273`) takes the same `resolve_exec_tool_worktree` → `run_exec_tool_locally` path, so the root is the session worktree.

**Managed, peer-owned agent.** `owned_agent_codebase_access` → `run_hosted_clone_tool`. Reads are served from the hosted clone. Mutations go to the facilitator's real worktree through `execute_tool_on_facilitator`.

**Containment.**
- `contain_path(worktree_root, arg)` confines file tools.
- `Shell` runs with `current_dir(root)` in its own process group. It is not otherwise confined (`lib.rs:615–617`).
- A sandboxed workspace session's jail mounts **only the worktree** read-write, plus its scratch dir (`workspace_tool_sandbox.rs:226`). A clone under the system tmp dir would not be reachable from the jail route.
- Choices for placing a clone: put it under the worktree, which pollutes it; put it under the session dir or the jail scratch dir; or add a second mount.

**How edits are applied.**
- Direct `std::fs::write` / `remove_file` after `contain_path`.
- StrReplace requires a unique match and returns `edited_region` / `edited_line` (±8 lines of the file after the edit, `edited_region.rs`).
- Write returns only `bytes_written`. Delete returns `deleted`.
- `exec_tool_service.rs` is just the RPC adapter over `ExecToolHandler`.

**Recording.** Every daemon-side call is appended to `tool_call_log` (`local_exec_tools.rs:144–163`).

### 4.3 Result summary and where the outcome returns to the caller (Q3)

- `summarize()` runs at the single append site, `run_one_turn` in `subagent.rs:1515`, and is stored on the `TranscriptEntry`.
- It is exposed through `MessageDescriptor.result_summary`, which reaches `PromptOutcome.messages` via `descriptors_from(appended_from)` in `take_turn:1920`.

**Serialization paths:**
- **Local loop in `tddy-tools`:** `run_turn` → `prompt_outcome_json` (`subagent_runtime.rs:426`) produces the MCP text result, returned from `take_a_turn` or `subagent_await`.
- **Daemon-run loop:** `agent_turn_frames(&outcome)` (`session-agents/service.rs`) produces `AgentConversationChunk.messages[].result_summary_json`. `conversation.rs::parse_message_descriptor` reads it back into a `PromptOutcome`, which then goes through the same `prompt_outcome_json`.

**What the new facts need:**
- Files and lines created/updated/deleted, plus the commit short hash.
- New `ResultSummary` fields, or a sibling descriptor field such as `commit`.
- Engine support: Write should report whether it created or overwrote a file, or else derive this from a `git diff --numstat` / `--name-status` after each commit, which covers Shell too.
- Proto fields on `AgentMessageDescriptor`, or reuse of the JSON-string `result_summary_json`.
- `yield_condition::fact_holds` and `KNOWN_TOOLS` would need extending if callers are to yield on these facts.

**Wire notes:**
- The remote wire drops `tool_calls[].arguments` today (`conversation.rs:448–462`, TODO).
- Descriptor size affects the final-frame budget (todo `2026-09-26-a-turns-message-list-can-overflow…`).

### 4.4 Message ids, rewind, replacement, cancel (Q4)

**Ids.**
- `m{n}` from a counter that never goes backwards (`transcript.rs:243`).
- The system prompt is `m1` when present.
- A replacement mints two ids (assistant `call_replacement_{n}`, then tool).

**Rewind.**
- Only `subagent_resume{fromMessageId}` → `TurnRequest::resuming().from_message(id)` → `take_turn` → `transcript.rewind_to` (`subagent.rs:1893`). This happens **before** anything is appended and before any model call.
- The cut extends forward over the tool results that answer the named call.
- An unknown id is an error and the history is left untouched.
- Rewind cannot be combined with a prompt; that combination is refused in `conversation.rs:227`.
- **Where the clone reset hooks in:** in `take_turn` right after `rewind_to` succeeds, resetting to the commit recorded on the last kept entry. That requires storing a commit sha on `TranscriptEntry` at `push_tool_result` time, and ideally a baseline commit at open.

**Replacement.**
- Append-only and never dispatched. It creates no commit.
- If the replaced call was a mutation, the caller's claimed result has no matching files in the clone. Worth deciding in the PRD.

**Yield.** A fired condition stops the turn after the tool result is appended, so that call's commit would already exist. `yieldedMessageId` names the tool message.

**Cancel, MCP side** (`server.rs:2065`):
- Retires the conversation, which drops the `Box<dyn SubagentSession>` including its transcript.
- Answers running and queued turns with the error "conversation cancelled" and aborts the tokio task (`subagent_runtime.rs:367–383`). A turn can be killed between a tool write and its commit.
- Closes the roster entry, rewrites the accounting file, and calls `CancelAgentConversation` for remote conversations.

**Cancel, daemon side:** `cancel_agent_conversation` removes the conversation and calls `closed.notify_one()`. The in-flight turn's `select!` returns `failed_precondition`.

**What is discarded:** the whole history and the running turn. Token accounting is kept in `retired`. There is no filesystem rollback today, because edits already landed in the real worktree.

**The feature needs:**
- "Cancelled means don't pull" semantics.
- A clean-up rule for the clone directory, which should hook into `retire` and the daemon's `conversations.remove`.
- Detach-driven cancel follows the same `retire` path (`server.rs:1959–1981`).

### 4.5 Existing git helpers and the "clone" concept (Q5)

**CLI only.** Every crate uses `std::process::Command::new("git")` or the tokio equivalent. Nothing depends on `git2` or `gix`.

**What the peer clone is:**
- A `workspace` session checkout on a **peer daemon (B)**, one per (session, owning daemon), shared by every agent B owns.
- A **one-way read mirror** of the facilitating daemon's (A) session worktree:
  - fetches `refs/tddy/session/{id}/wip`
  - `reset --hard <wip>^` then `read-tree -u --reset <wip>`
  - applies `StreamAgentActivityDelta` patches with `git apply`
- Any local modification is logged as a divergence and restored away.
- Mutating tools are **proxied to A's authoritative worktree** (`execute_tool_on_facilitator`).
- State is pushed with `ReportAgentCloneState`: `PROVISIONING → READY/ERROR`, `worktree_path`, `divergences`.
- `agent_clone_worktree.rs` only turns a reported path into a value or a `failed_precondition`.

**It is not an isolation mechanism.** It is shared, read-only, per-host, and gets overwritten. The new per-conversation writable clone is a different primitive.

**Reusable code:**

| Need | Existing code |
|---|---|
| Snapshot the tree as a commit without touching the index | `session_room::write_wip_tree_within` (scratch `GIT_INDEX_FILE` seeded from the real index), then `publish_wip_ref` (`commit-tree -p HEAD` with a fixed daemon identity, `update-ref`). A lazy-clone seed could base itself on the caller's uncommitted state the same way. |
| Diff between two commits | `session_room::diff_between` (`--binary --no-ext-diff --no-textconv`), `changed_paths_between` (`-z`) |
| Per-call file/line stats | `tddy_worktree_service::worktrees::parse_git_diff_numstat` |
| Apply into the caller's tree | `tddy_session_sync::Mirror::apply`'s `apply --check` then `apply` on stdin, with its three-way `ApplyOutcome`. `CloneMirror::git_output` is a small async runner. |
| Git runner with a deadline, `GIT_OPTIONAL_LOCKS=0`, null stdin | `session_room::git_output` (private) |
| Fetch between repos | `CloneMirror::restore` (`fetch <src> +ref:local`); the `tddy-remote-git-repo` `GIT_SSH_COMMAND` shim for cross-host |
| Worktree add/remove/list | `tddy_git::worktree` |
| Test repo | `tddy_testing_commons::fs::temp_dir_with_git_repo` |

**Missing:** there is no `git clone --local/--shared` helper for production code, and no per-call commit helper.

### 4.6 The `subagent_*` MCP tools (Q6)

All live in `packages/tddy-tools/src/server.rs`. Each returns a JSON string inside `CallToolResult::success(text)`. Errors are `{"error", "is_error": true}` (`subagent_error_json`).

| Tool | Input | Output |
|---|---|---|
| `subagent_new_session` | `agent` (enum from roster), `sessionId?`, `cwd?` (hint, unused), `systemPrompt?` (2375) | `{sessionId}` |
| `subagent_prompt` | required `sessionId`, `prompt:[{type,text}]`; optional `graceMs`, `maxTurns`, `yieldConditions` (2441) | outcome (below), or `{responseId, pending:true, queuePosition, queueSize, provider, providerQueuePosition, providerQueueSize}` |
| `subagent_resume` | required `sessionId`; optional `fromMessageId`, `correction`, `graceMs`, `maxTurns` (2472). The parser also accepts `yieldConditions` and `replacement`, which the schema does not advertise. | same as prompt |
| `subagent_await` | required `responseId`; optional `timeoutMs` (2574) | same outcome or pending shape |
| `subagent_cancel` | `sessionId` | `{cancelled: bool}` |
| `subagent_list` | none | `{conversations:[{agent,id,model,inputTokens,outputTokens,totalTokens,turns,contextTokens?,queued?}]}` |
| `subagent_status` | `agent?`, `waitFor:"ready"?`, `timeoutMs?` | `{sessionId, appliedRev, agents:[{agentId,label,model,daemonInstanceId,status,cloneState,replaces,conversations,lastActivity?}], timedOut?}` |

The outcome shape is `{stopReason, content, usage:{inputTokens,outputTokens,totalTokens}, messages:[{id,role,tool,toolCalls,isError,preview,resultSummary?}], clampedMaxTurns?, firedCondition?, yieldedMessageId?}`.

**Adding new tools** (pull a commit range, diff two commits):
- Add a route in `subagent_tool_router()`. `subagent_tool_names()` picks it up automatically for advertisement.
- For daemon-run conversations, each needs a new `SessionAgentService` RPC, a proto message, a `RemoteAgentSession`/link method, an `IN_JAIL_RELAYABLE` entry, and an update to its pinned test.
- Or both could ride an extended `SubagentSession` trait.

### 4.7 Test suites and helpers (Q7)

**`packages/tddy-discovery/tests/`, 31 files.** Subagent-turn suites:
- `subagent_session_red.rs`, `subagent_loop_red.rs`, `subagent_resume_red.rs`, `subagent_message_ids_red.rs`
- `tool_result_summary_acceptance.rs`, `yield_conditions_acceptance.rs`, `resume_replacement_acceptance.rs`
- `subagent_write_tools_red.rs`, `subagent_tool_outage_red.rs`, `subagent_turn_budget_red.rs`, `subagent_context_exhaustion_red.rs`
- `subagent_tool_argument_validation.rs`, `subagent_tool_call_arguments.rs`, `repeated_tool_calls.rs`, `codebase_access_red.rs`
- `read_window_red.rs`, `read_output_cap_red.rs`, `subagent_search_result_cap.rs`, `grep_context_local_acceptance.rs`
- `subagent_usage_red.rs`, `subagent_generation_cap.rs`, `subagent_provider_*`, `provider_queue.rs`
- `subagent_system_prompt_override.rs`, `subagent_replaced_tools_acceptance.rs`, `subagent_tool_exec_catalog_red.rs`, `specialized_*`, `agent_def_red.rs`, `no_builtin_agents_acceptance.rs`

**How they work.** Each file defines its own helpers rather than sharing them:
- `a_def(base_url[, max_turns])`
- `a_codebase_that_answers*` / `a_codebase_answering_*` (a Managed closure returning a canned result)
- `managed_access_with(response) -> (RecordedCalls, CodebaseAccess)` in `subagent_write_tools_red.rs:20`, which records the dispatched `(tool, args)`
- `a_codebase_whose_jail_is_dead`
- `a_turn_that_calls(tool, args)` / `a_turn_that_reads(path)`, `a_final_answer(text)`
- `a_model_that_…` (wiremock `POST /v1/chat/completions` with `.up_to_n_times(1)` then a fallback)
- `a_session_over(server)`, `a_yielded_conversation_over`
- `histories_the_model_received(server)` / `the_last_history_the_model_received` (from `server.received_requests()`)
- `the_id_of_the_first(outcome, role)`, `the_tool_result(outcome)`, `roles(outcome)`

Dev-dependencies: `tokio`, `wiremock 0.6`, `tempfile`, `tddy-testing-commons`.

**Gap for this feature.** No discovery test drives the real tool engine against a git repo; mutation tests only record the dispatched arguments. A clone/commit acceptance test would pair `temp_dir_with_git_repo` with a Managed closure over `tddy_tool_engine::execute_tool(clone_root, …)`. `tddy-discovery` does not depend on `tddy-tool-engine` today, so that needs a dev-dependency.

**`packages/tddy-tools/tests/`.**
- Suites: `subagent_mcp_acceptance.rs`, `subagent_resume_mcp_acceptance.rs`, `subagent_async_response_acceptance.rs`, `subagent_multi_agent_mcp_acceptance.rs`, `subagent_status_wait_acceptance.rs`, `subagent_system_prompt_mcp_surface.rs`, `subagent_token_accounting_acceptance.rs`, `subagent_tool_advertisement_acceptance.rs`, `session_agent_conversation_client_acceptance.rs`, `session_agent_roster_client_acceptance.rs`, `roster_before_tool_list_acceptance.rs`, `mcp_tool_advertisement_audit.rs`.
- Helpers: `spawn_mcp_server(_in)`, `send_json_line`, `read_json_line`, `initialize_mcp_session`, `call_tool`, `tool_result_json`, `tools_list`, def JSON through `TDDY_SUBAGENTS_JSON`.
- With no transport these run `CodebaseAccess::Local`, which refuses writes. A clone test at this level needs `TDDY_SUBAGENT_CODEBASE_ACCESS=managed` plus a transport stub, or a new mode.

**Daemon side.**
- `packages/tddy-session-agents/tests/agent_conversation_system_prompt.rs` (fake ports), `agent_session_status_inference_unit.rs`.
- `packages/tddy-session-lifecycle/tests/`: `split_session_roster_routing_acceptance.rs`, `session_sync_livekit_acceptance.rs`, `agent_*`.
- `packages/tddy-session-sync/tests/mirror_acceptance.rs` and `sync_acceptance.rs` contain git fixtures: `git_with_index`, `commit-tree`, `diff --binary --cached`.

**Known gap.** No wire-level test covers `ResumeAgentConversation` (todo `2026-09-26-the-resume-rpc-and-its-turn-budget-have-no-wire-level-test.md`).

### 4.8 Docs on clones, commits, isolation (Q8)

**`docs/ft/coder/specialized-subagents.md`** and **`docs/ft/coder/managed-codebase-subagents.md`:** no mention of clones, commits, or isolation (grep empty).
- `managed-codebase-subagents.md` § Turn control (171–335) specifies `messages`, `resultSummary`, yield, resume/rewind, replacement, and the honest-failure guard. It is the section the new behaviour extends.

**`docs/ft/daemon/session-agent-roster.md` § Clones (519–607):**
- one clone per (session, remote daemon), shared
- never the project directory or a worktree A owns
- one-way mirror
- "Reads are local; writes proxy" table
- `SHELL` proxy is called "the sharp edge"

**Direct conflicts with the planned feature, which the PRD will need to change:**
- Design decision "Reads local, writes proxied" (1076–1081): "Chosen over a read-write clone with write-back because the session has exactly one authoritative worktree and a second writer needs a conflict story that nothing in this system has."
- Non-goals (1098, 1105): "**Write-back from a remote clone.** Mutations proxy; they do not reconcile." and "**Per-agent clone isolation on one host.** Agents owned by one daemon share its clone."

**Relevant `docs/dev/todo` items:**
- `2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it.md`: an authorization gap that new pull/diff RPCs would inherit.
- `2026-09-26-a-jail-rebuild-can-re-run-a-tool-call-that-already-executed.md`: at-least-once mutating calls, so a per-call commit may record a double write.
- `2026-09-26-a-turns-message-list-can-overflow-the-chunk-framing-threshold.md`: a size budget for any new descriptor field.
- `2026-09-26-seven-files-over-budget…`, `2026-09-28-conversation-rs-crossed-the-file-budget.md`, and `packages/tddy-discovery/docs/code-issues/oversized-file-{subagent,subagent-runtime,conversation}.md`. `subagent.rs`, `server.rs`, `session-agents/service.rs`, `subagent_runtime.rs`, and `roster/conversation.rs` are all over the 500-line budget, so new logic should go in new modules (for example `subagent/workspace_clone.rs`).
- `2026-09-27-a-subagent-turn-has-no-wall-clock-deadline.md`.
- `2026-09-26-the-resume-rpc-and-its-turn-budget-have-no-wire-level-test.md`.

**Nothing in `docs/dev/todo` or `docs/dev/1-WIP` already covers subagent commit or clone isolation.**

### 4.9 Seams for the design (a summary of what the code implies)

1. **Where to intercept.**
   - The cleanest single seam is wrapping `CodebaseAccess::Managed` so its dispatch runs against a lazily created clone root and commits after each fail-closed mutating call.
   - The alternative is adding state to `SpecializedSubagentSession`. Its `access` is conversation-scoped, and the transcript append site in `run_one_turn` is where a commit sha can be attached to the entry and summary.
   - Both host sites build the access themselves: `tddy-tools` `mcp_primitives::managed_codebase_access`, and the daemon's `local_agent_codebase_access` / `owned_agent_codebase_access`.
   - In-jail Managed dispatch cannot choose a root because the `ExecuteTool` envelope has no conversation id. A clone there needs either a new envelope field and daemon-side root resolution, or the clone living daemon-side with the daemon running the loop. The jail case is every conversation, since the jail holds no defs.
2. **Rewind hook:** `take_turn` right after `rewind_to`, reset to the last kept entry's commit. The opt-out would be a new `TurnRequest` field plus a `subagent_resume` schema property plus a `ResumeAgentConversationRequest` field (next free number is 10).
3. **Turn-completion pull hook:** `subagent_runtime::run_turn` after `take_turn` returns, or the daemon `take_a_turn` spawn after `Ok(outcome)`. Cancel paths (`retire`, `closed.notify`, `abort`) must skip it.
4. **Jail constraint:** a sandboxed workspace jail mounts only the session worktree. A clone in the system tmp dir cannot be served by the jail route without a new mount or a host-worktree route.
5. **Classification:** reuse `agent_tool_reads_the_clone` semantics. It fails closed and counts `Await` as possibly mutating. It is currently `pub(crate)` in `tddy-session-lifecycle`, so it would need a shared home, for example next to `SubagentTool`.


## Exploration 2 — parent passes: jail relay, exec handler, conversation ids, backlog

### Sequence
1. `sed -n 40,130p packages/tddy-sandbox-runner/src/runner.rs` — how the jail relays `ExecuteTool`.
2. `grep -rn "message ExecuteToolRequest" -A25 packages/*/proto/*.proto` — the envelope fields.
3. `grep -n IN_JAIL_RELAYABLE -A15 packages/tddy-service/src/session_agents.rs` — the family-B allowlist.
4. `grep -rn "ExecToolHandler for" packages` — the single daemon implementor.
5. `grep -n "trait ExecToolHandler" -A30 packages/tddy-tool-engine/src/exec_tool_service.rs`.
6. `grep -n tmp .gitignore; git check-ignore -v tmp/x` — `tmp` is ignored in this repo (line 78), not guaranteed in a user's repo.
7. `grep -n sessionId packages/tddy-tools/src/server.rs` — conversation id origin.
8. Code-issue scan for tddy-daemon-rpc, tddy-session-lifecycle, tddy-sandbox-runner, tddy-service, tddy-session-tool-client, tddy-git; `grep -rl 'Claimed by' packages/*/docs/code-issues/`.
9. Read the heads of todo `2026-09-26-a-jail-rebuild-can-re-run-a-tool-call-that-already-executed.md`,
   `2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it.md`,
   `2026-09-26-a-turns-message-list-can-overflow-the-chunk-framing-threshold.md`,
   `2026-09-26-seven-files-over-budget-deferred-by-the-subagent-turn-control-change.md`.

### Excerpts
`runner.rs:71-80` — the relay forwards only two fields:
```rust
if service == tddy_tool_engine::EXEC_TOOL_SERVICE && method == "ExecuteTool" {
    let req = match ExecuteToolRequest::decode(message.payload.as_ref()) { … };
    let resp = self.relay.call_tool(&req.tool_name, &req.args_json).await;
```
Every other RPC must be in `IN_JAIL_RELAYABLE` (six family-B pairs) or it is `not_found`.

`exec_tools.proto:52-58`:
```proto
message ExecuteToolRequest {
  string session_token = 1; string session_id = 2; string tool_name = 3;
  string args_json = 4; string daemon_instance_id = 5;
}
```
`ExecToolHandler` (`exec_tool_service.rs:16-36`): `execute_tool`, `stream_execute_tool`,
`list_exec_tools`, `list_session_tool_calls`; sole impl `tddy-daemon-rpc/src/exec_tool/ports.rs:31`.

`server.rs:1707`: "the caller decides the conversation id; one is generated" otherwise.

### Findings
- The new `ConversationWorktree` RPC must be added to the jail's forward list (a new
  exec-tools allowlist pair next to `IN_JAIL_RELAYABLE`, or `ExecuteTool`'s own special-case), and
  `ExecuteTool`'s relay must carry `conversation_id`.
- No code issue in any touched package carries `Claimed by:`. `tddy-git` has no `docs/code-issues/`
  (not analyzed). `tddy-daemon-rpc`'s `complexity-exec-tool-ports-stream-execute-tool.md` is in the
  path (unclaimed; nesting 5).
- The conversation-id authorization gap (todo) does not transfer: the worktree path is derived under
  the **token-resolved** session worktree, so a foreign conversation id can only ever name a
  directory inside the caller's own session.
