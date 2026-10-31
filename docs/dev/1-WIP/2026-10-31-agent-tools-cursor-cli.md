# Changeset: Cursor CLI sessions launch from cursor.yaml, on the host and in the jail

**Created:** 2026-10-31
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — `tddy-coding-agent-specs`, `tddy-agent-launch`, `tddy-sandbox-recipes`, `tddy-tools` hooks routing; a hook transport over the jail bridge; live jail egress
**Stack:** `#agent-tools 7/12` — branch `feature/agent-tools/cursor-cli`, base `feature/agent-tools/claude-headless`
**PR:** [#796](https://github.com/uppin/tddy-coder/pull/796)
**PRD:** [PRD-2026-10-31-cursor-cli-from-spec.md](../../ft/daemon/1-WIP/PRD-2026-10-31-cursor-cli-from-spec.md)

## Initial Discovery

[2026-10-31-agent-tools-cursor-cli-initial-discovery.md](./2026-10-31-agent-tools-cursor-cli-initial-discovery.md)

State A below is distilled from that file. Grep traces and file dumps are not repeated here.

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-10-17-desktop-cursor-hooks-reach-daemon.md](../todo/2026-10-17-desktop-cursor-hooks-reach-daemon.md) | ✅ **RESOLVED HERE** (decided 2026-10-31). The hooks slot renders the daemon's agent-tool socket on the host, and hook reports go over the tool-IPC bridge in a jail |
| `packages/tddy-agent-launch/docs/code-issues/crap-svc-start-sandboxed-cursor-cli-session.md` | ✅ **RESOLVED HERE.** The function is deleted, and the jailed Cursor start is executed by the new acceptance tests. Claimed at commit 2 |
| `packages/tddy-agent-launch/docs/code-issues/complexity-cursor-cli-spawn-spawn-cursor-cli-session-inner.md` | ✅ **RESOLVED HERE.** The per-agent body is replaced by `launch_host_pty`. Claimed at commit 2 |
| [2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md](../todo/2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md) | ℹ Claimed by #794 claude-jail. This node moves the third caller (Cursor) onto `launch_jailed`, and #794's wrap records the closure |
| [2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md](../todo/2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md) | ⚠ During. Unchanged: `cursor.yaml` declares `tool_deny: unsupported`, so the refusals stay spec-driven |
| [2026-10-25-cursor-project-mcp-config-outlives-its-conversation.md](../todo/2026-10-25-cursor-project-mcp-config-outlives-its-conversation.md) | — Unrelated here (headless conversations). Claimed by #798 |

## Affected Packages

- [ ] [`tddy-coding-agent-specs`](../../../packages/tddy-coding-agent-specs/README.md): `specs/cursor.yaml`; the `cursor-mcp-json` format; the `cursor-hooks-json` hooks renderer; a `rules_file` instruction kind
- [ ] [`tddy-agent-launch`](../../../packages/tddy-agent-launch/README.md): host and jailed Cursor starts and resumes call the shared launchers; per-agent bodies deleted; hooks get the daemon socket
- [ ] [`tddy-sandbox-recipes`](../../../packages/tddy-sandbox-recipes/README.md): nothing new: Cursor's install-tree reads come from spec `files`/`reads`
- [ ] [`tddy-sandbox-runner`](../../../packages/tddy-sandbox-runner/README.md): hook reports are relayed over the tool-IPC bridge (generic: any jailed `tddy-tools session-hook`)
- [ ] [`tddy-tools`](../../../packages/tddy-tools/README.md): `session-hook` uses the bridge when `TDDY_SANDBOX_TOOL_IPC` is set

## Responsibility

- `cursor.yaml`:
  - `binary { command: agent, env: TDDY_CURSOR_AGENT }`;
  - the install layout as `files: Mirror` plus `run.argv` (`node index.js` when the resolved binary is a launcher script);
  - argv `--resume {chat_id} --model {model} {prompt}`;
  - `mcp: { kind: file, path: "{home}/.cursor/mcp.json", format: cursor-mcp-json }`;
  - `instructions: { rules_file: .cursor/rules/tddy-managed-workflow.mdc }`;
  - `hooks: { adapter: cursor-hooks-json }`;
  - `tool_deny: unsupported`;
  - `home: { kind: persistent, key: cursor }`;
  - credential `SeedOnce` items;
  - env `CURSOR_SKIP_KEYCHAIN=1`, `CI=true`, `CURSOR_TMPDIR`;
  - `network.hosts`.
- `McpFormat::CursorMcpJson`, `InstructionKind::RulesFile` and the `cursor-hooks-json` renderer, which takes a daemon-socket parameter.
- `spawn_cursor_cli_session` and `resume_cursor_cli_session` call `launch_host_pty`. `start_sandboxed_cursor_cli_session` calls `launch_jailed`.
- Hook delivery: the daemon socket on the host. In a jail, `tddy-tools session-hook` sends the report over `TDDY_SANDBOX_TOOL_IPC`, and the runner relays it as a `HostRpc`.
- The chat id is minted and persisted as the `{chat_id}` template parameter.

## Boundaries

- No headless Cursor (#798) and no standalone app (#799).
- The runner's `spawn_cursor_pty` and `--agent-kind cursor` remain for `tddy-sandbox-app` until #799.
- Managed-codebase enforcement for Cursor is unchanged.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n2` claude-host | `launch_host_pty`, slots `render`, `CodingAgentSpec` | Cursor's host start and resume | change slot kinds beyond adding Cursor's formats; touch `claude.yaml` |
| `n5` claude-jail | `launch_jailed`, the generic `home`, `files`, `secrets` and `policy`, `--exec-plan` | Cursor's jailed start | change the launcher's contract or the home migration |
| `n3` egress-allowlist | `network` and enforcement | Cursor's declared hosts; the refusal test | change `admits` |
| `n1` tool-defs (#790) | registry and resolution | resolves `agent` | — |

Adding `CursorMcpJson`, `RulesFile` and `cursor-hooks-json` extends #791's enums additively. The existing variants are untouched.

## Draft PR contract

The first push publishes `cursor.yaml`, the new format and instruction variants, the hooks-renderer signature with the socket parameter, the bridge hook-report message, and the failing tests below. Unimplemented bodies are `// TODO(cursor-cli): implement`.

## Green wave

**Wave:** 4 of 7
**Greenable independently:** after #794 (`launch_jailed`) and #792 (egress) are green
**Concurrent with:** #797 harness-clis
**Blocks:** #798 headless-others (`cursor.yaml`'s headless section) and #799 jail-agnostic (Cursor in the app)

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4      n2 → n5, n6      n3 → n5, n7, n8      n5 → n7, n8, n10
    n6 → n9      n7 → n9, n10      n8 → n10, n11      n9 → n11      n2..n11 → n12

## State A

- `cursor_cli_spawn.rs` (`spawn_cursor_cli_session_inner`, over budget), `svc_start_sandboxed_cursor_cli_session.rs` (CRAP, never executed) and `SandboxRecipe::CursorCli` (install-tree reads, credential copies, env overlay).
- The runner's `spawn_cursor_pty` rewrites argv to `node index.js` and writes `$HOME/.cursor/mcp.json`.
- `cursor_hooks.rs` writes `.cursor/hooks.json` with `daemon_socket: None`, so desktop hooks hit a 404.
- The Claude overlay is applied to Cursor jails.

## State B

- Host and jailed Cursor sessions render from `cursor.yaml` through the shared launchers.
- Hooks reach the daemon on the desktop host and from inside the jail.
- No Cursor flag reaches the runner from the daemon.

## Delta

### New
- `packages/tddy-coding-agent-specs/specs/cursor.yaml`
- `packages/tddy-coding-agent-specs/src/slots/cursor.rs`
### Modified
- `tddy-agent-launch` `cursor_cli_spawn.rs` and `chat.rs` (hooks).
- `tddy-tools` `session_hook.rs`; `tddy-sandbox-runner` (generic hook relay).
- Docs at wrap: `docs/ft/daemon/cursor-cli-session.md`.
### Removed
- `packages/tddy-agent-launch/src/svc_start_sandboxed_cursor_cli_session.rs`, and the per-agent body of `spawn_cursor_cli_session_inner`

## Testing Strategy

### Acceptance tests

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/cursor_cli_from_spec_acceptance.rs`**

- [x] `a_host_cursor_cli_start_and_resume_spawn_a_stub_agent_whose_argv_and_cwd_match_the_spec_and_register_no_mcp_server_in_the_users_home` (renamed at red: see `### From /red`)
- [x] `a_jailed_cursor_cli_start_runs_the_stub_with_the_persistent_cursor_home_credentials_and_keychain_skip_env_and_no_claude_overlay` (macOS, real Seatbelt)
- [x] `cursors_jail_reaches_its_declared_hosts_and_is_refused_an_undeclared_one`
- [x] `the_runner_receives_no_cursor_specific_flag_on_the_daemon_path`
- [x] `on_an_embedded_daemon_a_host_cursor_sessions_hook_updates_its_activity_and_status`
- [x] `a_jailed_cursor_sessions_hook_reaches_the_daemon_through_the_bridge`

### Unit / integration tests

- [x] `tddy-coding-agent-specs` `slots::cursor`: the `cursor-mcp-json` shape; the rules file content and path; `hooks.json` carries the socket when given and is refused without one (no silent `daemon_url`).
- [x] `tddy-coding-agent-specs` `tests/cursor_spec_acceptance.rs`: `cursor.yaml`'s `coding_agent` section deserializes with the placed MCP, rules file and hooks adapter; the built-in registry holds `cursor` beside `claude`.
- [x] `tddy-agent-launch` `cursor_from_spec::tests`: the template parameters (`chat_id`, `model`, `prompt`, `node_launcher`/`node_entry`) and launcher-script detection.
- [x] `tddy-tools` `session_hook::tests`: hook transport selection — the tool-IPC socket in a jail, the daemon socket on the host, HTTP otherwise.
- [x] `tddy-sandbox-runner` `tests/hook_report_relay.rs`: the in-jail socket relays both hook reports and still the roster RPCs, and not `AwaitQuestionAnswer`.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Launch output | Integration with a stub `agent` | Behaviour-preserving |
| Jail and hooks over the bridge | Live macOS Seatbelt | The bridge path only exists in a real jail |

## Decisions & Trade-offs

- **Hook reports ride the existing tool-IPC bridge in a jail.** No host path is handed to the jail, which is the reason the todo deferred the fix.
- **A hooks renderer without a socket is an error, not a fallback** to `daemon_url`. That fallback is what produced the silent 404.

## TODO

- [x] Record initial discovery (`2026-10-31-agent-tools-cursor-cli-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests — waived by the developer 2026-10-31: pass 2 runs without per-node review
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap

## Refactoring Needed
### From /red (TDD red phase)

**Red baseline (2026-10-31).** Scoped runs: `tddy-coding-agent-specs`, `tddy-service`, `tddy-tools --lib`, `tddy-sandbox-runner --lib --test hook_report_relay`, `tddy-agent-launch --lib`, and `tddy-session-lifecycle` (`cursor_cli_from_spec_acceptance`, `cursor_cli_session_acceptance`, `cursor_cli_hooks_acceptance`, `cursor_cli_resume_chat_acceptance`, `sandboxed_cursor_cli_acceptance`).

- **Fails for this node's reason** (a `todo!("cursor-cli: …")` or this node's YAML):
  - `slots::cursor::tests`, 8 tests: `render_cursor_mcp_json`, `placed_path`, `render_rules_file`, `render_cursor_hooks_json`, `CursorHooksJson::try_render`.
  - `cursor_from_spec::tests`, 5 tests: `cursor_template_params`, `node_launcher_of`.
  - `session_hook::tests`, 3 tests: `hook_transport`.
  - `hook_report_relay`, 3 tests: `relays_to_the_host`.
- **Sequencing facts** (a parent's `todo!`):
  - All 6 `cursor_cli_from_spec_acceptance` tests panic in `claude-host: load_agent_registry` (#791).
  - Both `cursor_spec_acceptance` tests (`cursor_yamls_coding_agent_section_declares_…_and_no_tool_flags`, `the_builtin_registry_holds_cursor_beside_claude…`) panic in the same `todo!` (#791). Both read the built-in registry, and `cursor.yaml` is listed in `builtin_specs()`.
  - Behind those: `launch_host_pty` and `HostCodingAgents::resolve` (#791), `launch_jailed` (#794), and `Egress::deserialize`/`admits` for the egress test (#792).
- **Pre-existing, not touched:**
  - `sandboxed_cursor_cli_acceptance` (4 tests) panics "sandbox RPC bridge not installed" on this Mac. The file is identical to `origin/master`; it is the known macOS harness gap.
  - The parents' own `todo!` failures in touched packages:
    - specs `slots::tests`, `headless::tests`, `spec_load_acceptance` and `headless_render_golden`;
    - runner `exec_plan`/egress unit tests (`claude-jail`, `egress-allowlist`).
- **Still green:** the existing cursor suites (`cursor_cli_session_acceptance` 13, `cursor_cli_hooks_acceptance` 3, `cursor_cli_resume_chat_acceptance` 8), `tddy-service` (all), and the `tddy-tools`/`tddy-agent-launch`/runner libs apart from the tests above.

**Deviations from the planned surface, and why:**

- `mcp` is `kind: placed` (new `McpSlot::Placed { path, format }`), not `kind: file, path:`. #791's `File` variant requires `flag`, and changing it would change a parent symbol.
- `InstructionKind::RulesFile` is the field `InstructionsSlot.rules_file: Option<PathBuf>` (serde default). #791 has no `InstructionKind` enum. The rules file's content is the launch's `preamble`, written whole; no `SlotInputs` field was added for it.
- Additive parent-type changes:
  - `SlotInputs.home: Option<PathBuf>`, which `{home}` resolves to;
  - `SlotError::{NoDaemonSocket, UnboundHome}`;
  - a provided `HooksAdapter::try_render` (default `Ok(render(..))`), so `cursor-hooks-json` can refuse params without a socket while the trait stays as #791 published it.
  - Two literals in #791's `slots::tests` gained `rules_file: None` / `home: None`.
  - #791's `an_unknown_hooks_adapter_is_refused_naming_it` now uses `an-unregistered-hooks-json`, since this node registers `cursor-hooks-json`.
- The bridge message is the constant `tddy_service::session_activity::IN_JAIL_RELAYABLE_HOOK_REPORTS` (`ReportSessionStatus`, `ReportAgentActivity`), carried by the existing `RpcRequest` frames. No new proto message or RPC.
  - `tddy_sandbox_runner::relays_to_the_host` is the allowlist green switches `ToolExecService` to.
  - `tddy_tools::session_hook::{HookTransport, hook_transport}` picks the tool-IPC socket in a jail.
- The host acceptance test is renamed `…_whose_argv_and_cwd_match_the_spec_and_register_no_mcp_server_in_the_users_home`. Writing `{home}/.cursor/mcp.json` on the host would overwrite the user's own global Cursor config. Today's host launch writes none, and Claude's host launch registers no MCP server either. The jailed test asserts the registration in `homes/cursor`. **Developer decision:** keep no MCP registration on the host, or use a project-level `<worktree>/.cursor/mcp.json`.
- Unit tests beyond the plan: `cursor_from_spec`, `session_hook` transport, `hook_report_relay`, `cursor_spec_acceptance`.
- The fixture `tests/cursor_cli_support/mod.rs` duplicates the shape of #794's `claude_jail_support`, which is parent-owned and Claude-specific. That duplication is a refactor candidate once both suites are green.
- `cursor.yaml`'s `models` list and its commented `network.hosts` are unverified; confirm both at green.

**Parent decision taken (#791 commit 3, `5679e8612`):**

- `CodingAgentSection.{tool_allow, tool_deny, permission_prompt}` are `ToolSlot = Flag(FlagSlot) | Unsupported`. The YAML word `unsupported` selects the second form.
- `slots::render` refuses a filled `unsupported` slot with `SlotError::SlotUnsupported`.
- `cursor.yaml` declares all three `unsupported` and is now listed in `builtin_specs()`.

**Open question for green:** a non-embedded (server) daemon serves no agent-tool socket. Under "no socket is an error", a host Cursor session there gets no hooks file. Decide whether a server daemon serves the socket too, or whether that case is refused at start.

**`TODO(...)` deletions needing approval (CLAUDE.md § ASK):** green deletes `svc_start_sandboxed_cursor_cli_session.rs` and the per-agent body of `spawn_cursor_cli_session_inner`. Both are marked in the source.
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |
