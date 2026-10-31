# Changeset: Cursor CLI sessions launch from cursor.yaml, on the host and in the jail

**Created:** 2026-10-31
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — `tddy-coding-agent-specs`, `tddy-agent-launch`, `tddy-sandbox-recipes`, `tddy-tools` hooks routing; a hook transport over the jail bridge; live jail egress
**Stack:** `#agent-tools 7/12` — branch `feature/agent-tools/cursor-cli`, base `feature/agent-tools/claude-headless`
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
| [2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md](../todo/2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md) | ℹ Claimed by #5 claude-jail. This node moves the third caller (Cursor) onto `launch_jailed`, and #5's wrap records the closure |
| [2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md](../todo/2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md) | ⚠ During. Unchanged: `cursor.yaml` declares `tool_deny: unsupported`, so the refusals stay spec-driven |
| [2026-10-25-cursor-project-mcp-config-outlives-its-conversation.md](../todo/2026-10-25-cursor-project-mcp-config-outlives-its-conversation.md) | — Unrelated here (headless conversations). Claimed by #9 |

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

- No headless Cursor (#9) and no standalone app (#10).
- The runner's `spawn_cursor_pty` and `--agent-kind cursor` remain for `tddy-sandbox-app` until #10.
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

Adding `CursorMcpJson`, `RulesFile` and `cursor-hooks-json` extends #2's enums additively. The existing variants are untouched.

## Draft PR contract

The first push publishes `cursor.yaml`, the new format and instruction variants, the hooks-renderer signature with the socket parameter, the bridge hook-report message, and the failing tests below. Unimplemented bodies are `// TODO(cursor-cli): implement`.

## Green wave

**Wave:** 4 of 7
**Greenable independently:** after #5 (`launch_jailed`) and #3 (egress) are green
**Concurrent with:** #8 harness-clis
**Blocks:** #9 headless-others (`cursor.yaml`'s headless section) and #10 jail-agnostic (Cursor in the app)

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

- [ ] `a_host_cursor_cli_start_and_resume_spawn_a_stub_agent_whose_argv_cwd_and_mcp_json_match_the_spec`
- [ ] `a_jailed_cursor_cli_start_runs_the_stub_with_the_persistent_cursor_home_credentials_and_keychain_skip_env_and_no_claude_overlay` (macOS, real Seatbelt)
- [ ] `cursors_jail_reaches_its_declared_hosts_and_is_refused_an_undeclared_one`
- [ ] `the_runner_receives_no_cursor_specific_flag_on_the_daemon_path`
- [ ] `on_an_embedded_daemon_a_host_cursor_sessions_hook_updates_its_activity_and_status`
- [ ] `a_jailed_cursor_sessions_hook_reaches_the_daemon_through_the_bridge`

### Unit / integration tests

- [ ] `tddy-coding-agent-specs` `slots::cursor`: the `cursor-mcp-json` shape; the rules file content and path; `hooks.json` carries the socket when given and is refused without one (no silent `daemon_url`).

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
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
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
