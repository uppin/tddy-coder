# Initial Discovery: Project-home agent reaches its tools

**Changeset**: [2026-10-24-project-home-agent-tools.md](./2026-10-24-project-home-agent-tools.md)
**Date**: 2026-10-24
**Passes**: 3

## Combined Conclusions

**The failure.** Tddy Desktop session `01a16dc6-ab4c-7dc3-844e-baca4514f86e` (`session_role:
project-home`, unsandboxed `claude-cli`, embedded daemon) was asked to start a session and could not.
Its `claude` was launched with one `--allowedTools mcp__tddy-tools__<Tool>` per project-home tool,
but with **no `--mcp-config`** and **no `TDDY_*` / `TDDY_REMOTE_*` environment**, so no `tddy-tools`
MCP server ran and none of those tools existed in its tool list. The agent fell back to native Bash,
tried `tddy-tools spawn-conversation` (managed-workflow only: `TDDY_SOCKET not set`), and asked the
operator what "test session" meant. It had no prompt saying it is the project's main.

**Where the gap is.**
- `claude_cli_spawn_steps.rs::role_allowed_tools_args` is the only role-specific launch input; it
  yields `--allowedTools` and nothing else. Called at start (`spawn_claude_cli_process`) and resume
  (`svc_resume_claude_cli_session.rs:71`).
- `claude_cli_spawn.rs` adds per-session environment only for a managed recipe
  (`managed_claude_cli_launch` → `TDDY_SOCKET`); a main has no recipe.
- Native Claude tools (Bash, Edit, Write, …) are not withdrawn for a main, and `--strict-mcp-config`
  is not passed. The PRD's "read-only" guarantee is enforced only on the daemon's `ExecuteTool`
  route, which native tools never cross.
- No role prompt (`--append-system-prompt-file`) for a main.

**What already exists and is reused.**
- **Transport.** `tddy-tools --mcp` picks its daemon transport from env
  (`tddy_session_tool_client::detect_session_tool_transport`): `TDDY_REMOTE_SESSION_ID` +
  `TDDY_REMOTE_DAEMON_SOCKET` → unix socket (embedded host); `TDDY_REMOTE_SESSION_ID` +
  `TDDY_REMOTE_DAEMON_URL` → HTTP (binary host). It asks `ListExecTools` per `tools/list` with the
  session id, and the daemon answers the project-home catalog for a main.
- **Embedded socket.** An embedded daemon binds `agent_tool_socket_path(data_dir)` serving its full
  RPC roster (`tddy-daemon/src/runtime/build_context.rs:148`);
  `tddy_daemon_kernel::embedded_host_agent_tool_socket(data_dir)` returns it only when something
  listens. Hooks already use it (`claude_cli_spawn_steps.rs:166`).
- **Env builder.** `tddy_session_split::split_session::colocated_jail_tool_env(url, session_id,
  agent_token, socket)` builds the `RemoteToolEnv` (URL + optional socket, no LiveKit fields) — the
  exact shape a co-located main needs, with the main's **own** session id (its record holds the home
  worktree, so `ExecuteTool` resolves `HostWorktree`, which is correct for an unsandboxed main).
- **Agent credential.** `agent_credentials::mint_agent_session_token` (pub(crate)) mints a 24 h
  token under the *verified* caller; `SplitSessions::agent_session_token_for` wraps it (pub(crate)).
  The caller's own token lives 5 minutes, so forwarding it is wrong. `ExecuteTool`'s project-home
  route resolves the OS user from this token (`exec_tool/project_home.rs:131`).
- **Signer is always present in production.** The session host is mounted only when the runtime has
  `SessionTokens` (`tddy-daemon/src/runtime/mount.rs:66`, `mount_host.rs:215`), so minting cannot
  fail for want of a signer where a main can start. Test fixtures without a signer must gain one.
- **MCP config + native withdrawal.** `tddy_sandbox_recipes::write_claude_mcp_config(dir,
  tools_path, env)` writes `claude-mcp-config.json`; `split_session::NATIVE_FILESYSTEM_TOOLS` lists
  every native filesystem/shell tool; `agent_argv::split_claude_extra_args` shows the full split
  pattern (allow, disallow, permission-prompt tool, `--mcp-config`, `--strict-mcp-config`).
- **tddy-tools path.** `tddy_session_split::svc_resolve_tddy_tools_path::resolve_tddy_tools_path(config)`
  resolves an absolute toolchain path (a relative one silently breaks the MCP server).

**Test seams.**
- Unit: `claude_cli_spawn_steps/claude_cli_spawn_steps_tests.rs` already has `AnEmbeddedHost`
  (socket bound for real) and `ARegularHost` fixtures, and `allowed_tools_in(argv)`.
- Acceptance: `tddy-session-lifecycle/tests/ensure_project_home_session_acceptance.rs` over
  `project_home_support` (stub `claude` echoing `ARGV:` and `TDDY_SOCKET`). The fixture's daemon has
  no `SessionTokens` and a plain `"valid-token"`; it needs a signer and a signed caller token, as
  `sandboxed_codebase_placement_acceptance.rs` does (`this_daemons_session_tokens`,
  `a_caller_token`). The stub must also dump `TDDY_REMOTE_*`.

**Constraints.**
- `svc_resume_claude_cli_session::resume_claude_cli_session` is an open code issue (144 lines,
  regressed in #700). The role wiring must enter it as one call, not inline assembly.
- `docs/ft/daemon/` has no `1-OVERVIEW.md`; nothing to update there.
- The sandboxed main is refused today (`2026-10-24-sandboxed-main-needs-session-role.md`); this change
  covers the unsandboxed main only.

## Exploration 1: the failed session — 2026-10-24

**Agent**: parent Bash/Read
**Scope**: what the session did and why it failed

### Sequence

1. `ls ~/.tddy; find … -name '*01a16dc6…*'` — locate the session dir
2. Read `~/.tddy/sessions/01a16dc6…/.session.yaml`, `changeset.yaml`, `agent-activity.jsonl`
3. Locate and dump the Claude transcript `~/.claude/projects/-Users-mantasi-Code-tddy-coder--worktrees-51777f55-…-home/01a16dc6….jsonl`
4. `grep 01a16dc6… ~/.tddy/logs/daemon` — daemon view of the start

### Inspected files

#### `~/.tddy/sessions/01a16dc6…/.session.yaml`

```yaml
repo_path: /Users/mantasi/Code/tddy-coder/.worktrees/51777f55-…/home
session_type: claude-cli
managed_codebase: true
session_role: project-home
```

#### Transcript (agent's own findings)

```text
tddy-tools spawn-conversation … → TDDY_SOCKET not set; spawn_conversation not relayed
ps -o command= -p $PPID → claude --model opus --session-id 01a16dc6… --permission-mode auto
  --allowedTools mcp__tddy-tools__Read … mcp__tddy-tools__StartProjectSession … (no --mcp-config)
env | grep -i tddy → nothing
```

#### Daemon log

```text
14:58:18.959 [WARN] session 01a16dc6… starts without an account identity: the github account uppin has no `subject_id` recorded …
14:58:19.095 [INFO] EnsureProjectHomeSession: started main 01a16dc6… for project 51777f55…
15:00:21.625 [WARN] session_room: … could not broadcast an activity record: broadcast payload of 21660 bytes exceeds the 8192 byte limit
```

### Findings

The main's `claude` had allowlisted tools with no server behind them; no system prompt; native tools
intact. The two log warnings are unrelated to the failure (recorded as TODOs).

## Exploration 2: launch, resume and transport code — 2026-10-24

**Agent**: parent Bash/Read
**Scope**: `tddy-agent-launch`, `tddy-session-split`, `tddy-session-tool-client`, `tddy-daemon-kernel`, `tddy-daemon`

### Sequence

1. Grep `role_allowed_tools_args|mcp-config|TDDY_SOCKET` in `tddy-agent-launch/src`, `tddy-cli-sessions/src`
2. Read `tddy-agent-launch/src/claude_cli_spawn.rs` (whole start orchestration)
3. Read `tddy-agent-launch/src/claude_cli_spawn_steps.rs:260-420`
4. Read `tddy-tool-engine/src/project_home_tools.rs:1-80`, `tddy-tools/src/exec_tool_admission.rs:1-90`, `tddy-daemon-rpc/src/exec_tool/project_home.rs:1-60`
5. Read `tddy-session-split/src/agent_argv.rs:100-230`
6. Grep `fn session_exec_tool_answer`; read `tddy-session-tool-client/src/lib.rs:220-310`
7. Grep `TDDY_REMOTE_DAEMON_SOCKET|RemoteToolEnv`; read `svc_resume_claude_cli_session.rs:40-110`
8. Read `docs/ft/daemon/project-concept.md:155-200`
9. Grep `agent_tool_socket_path`; read `tddy-daemon-kernel/src/agent_tool_socket.rs`, `tddy-daemon/src/runtime/build_context.rs:130-180`
10. Read `tddy-session-split/src/split_session.rs:20-60,165-260`, `agent_credentials.rs:1-60`, `svc_start_sandboxed_codebase_session.rs:180-300`
11. Grep `fn agent_session_token_for|fn agent_tool_socket_for_embedded_host|fn session_tokens|fn resolve_tddy_tools_path`
12. Read `tddy-agent-launch/src/cli_branch_starts.rs:100-200`, `svc_start_claude_cli_session.rs:34-150`, `launch_ports.rs:84-124`
13. Grep `with_session_tokens` in `tddy-daemon/src`; read `runtime/mount_host.rs:190-225`
14. Read `tddy-agent-backend/src/backend/mod.rs:360-430` (`RemoteToolEnv::env_pairs`), `tddy-daemon-kernel/src/daemon_hook_urls.rs:9`

### Grep / glob

| Tool | Pattern | Scope | Notable hits |
|---|---|---|---|
| Grep | `role_allowed_tools_args` | `tddy-agent-launch/src` | `claude_cli_spawn_steps.rs:278,362`; `svc_resume_claude_cli_session.rs:13,71` |
| Grep | `"--mcp-config"` | `packages` (non-test) | `tddy-session-split/src/agent_argv.rs:184`; `tddy-sandbox-app/src/host_agent.rs` — none in `tddy-agent-launch` |
| Grep | `TDDY_REMOTE_DAEMON_SOCKET` | `packages` | consumer only (`tddy-session-tool-client/src/lib.rs:276`); produced via `RemoteToolEnv.daemon_socket` |
| Grep | `colocated_jail_tool_env` | `packages` | `split_session.rs:230`; callers `svc_start_sandboxed_codebase_session.rs:229`, `svc_spawn_split_agent.rs:198` |
| Grep | `agent_tool_socket_path` | `packages` | bound in `tddy-daemon/src/runtime/build_context.rs:151` (embedded only) |
| Grep | `with_session_tokens` | `tddy-daemon/src` | `runtime/mount_host.rs:215` |

### Inspected files

#### `packages/tddy-agent-launch/src/claude_cli_spawn_steps.rs`

```rust
pub(crate) fn role_allowed_tools_args(session_role: Option<&str>) -> Vec<String> {
    if session_role != Some(tddy_core::session_metadata::PROJECT_HOME_ROLE) {
        return Vec::new();
    }
    tddy_tool_engine::read_only::project_home_allowed_tools()
        .into_iter()
        .flat_map(|tool| ["--allowedTools".to_string(), tool])
        .collect()
}
…
    let handle = manager
        .start_with_options(
            …
            append_system_prompt_file.as_deref(),
            role_allowed_tools_args(Some(session_role)),
            env_extra,
            Some(os_user),
        )
```

```rust
    // An embedded daemon serves no HTTP RPC; its hooks report over this socket.
    let daemon_socket = tddy_daemon_kernel::embedded_host_agent_tool_socket(tddy_data_dir);
```

#### `packages/tddy-agent-launch/src/claude_cli_spawn.rs`

```rust
    // Managed-workflow wiring: … inject a per-session TDDY_SOCKET …
    let (managed, append_system_prompt_file, mut env_extra) =
        claude_cli_spawn_steps::managed_claude_cli_launch(…).await?;
    env_extra.extend(provider_route.map(|route| route.env).unwrap_or_default());
    env_extra.extend(config.claude_cli_renderer_env());
    claude_cli_spawn_steps::record_role_before_spawn(…)?;
```

`spawn_claude_cli_session_inner` has no `session_token` parameter; the caller
(`LaunchSessions::start_claude_cli_session`) does.

#### `packages/tddy-agent-launch/src/svc_resume_claude_cli_session.rs`

```rust
        // A role's tool set is not in a persisted argv, so a resumed agent is given it again.
        split_args.extend(role_allowed_tools_args(meta.session_role.as_deref()));
        …
        let mut env_extra = self.resumed_agent_env(
            (!is_split).then_some(identity.git_environment),
            split_env,
            session_token,
            &session_dir,
        )?;
```

#### `packages/tddy-session-tool-client/src/lib.rs`

```rust
    // An embedded daemon serves no HTTP, so it exports a socket instead of a URL. …
    if let (Some(session_id), Some(socket_path)) = (
        non_empty_env("TDDY_REMOTE_SESSION_ID"),
        non_empty_env("TDDY_REMOTE_DAEMON_SOCKET"),
    ) { return Some(SessionToolTransport::DaemonUds { … }); }
    …
    if let (Some(session_id), Some(daemon_url)) = (
        non_empty_env("TDDY_REMOTE_SESSION_ID"),
        non_empty_env("TDDY_REMOTE_DAEMON_URL"),
    ) { return Some(SessionToolTransport::DaemonHttp { … }); }
    None
```

#### `packages/tddy-session-split/src/split_session.rs`

```rust
pub const NATIVE_FILESYSTEM_TOOLS: &[&str] = &[
    "Read", "Write", "Edit", "MultiEdit", "NotebookEdit", "Bash", "BashOutput", "KillShell",
    "Glob", "Grep", "LS",
];
…
pub fn colocated_jail_tool_env(
    this_daemon_url: &str,
    checkout_session_id: &str,
    agent_session_token: &str,
    daemon_socket: Option<String>,
) -> RemoteToolEnv { /* url + socket + session + token; no LiveKit, no instance id */ }
```

#### `packages/tddy-session-split/src/agent_credentials.rs`

```rust
pub(crate) fn mint_agent_session_token(
    tokens: &SessionTokens,
    caller_token: &str,
) -> Result<String, Status> {
    let caller = verified_caller(tokens, caller_token)?;
    Ok(tokens.signer().mint(&caller, SPLIT_AGENT_TOKEN_TTL))
}
```

Refusal text reads "cannot wire a split session: …" — wrong subject for a main.

#### `packages/tddy-session-split/src/agent_argv.rs`

```rust
    let mcp_config = tddy_sandbox_recipes::write_claude_mcp_config(session_dir, Path::new(tddy_tools_path), &mcp_env)?;
    …
    args.push("--mcp-config".to_string());
    args.push(mcp_config.to_string_lossy().into_owned());
    args.push("--strict-mcp-config".to_string());
```

#### `packages/tddy-daemon/src/runtime/build_context.rs`

```rust
        if self.options.host == RuntimeHost::Embedded {
            self.tasks.agent_tool_socket = Some((
                crate::agent_tool_socket::agent_tool_socket_path(&self.identity.keys.tddy_data_dir),
                cloned_entries(&self.identity.rpc_entries),
            ));
        }
```

#### `packages/tddy-daemon-rpc/src/exec_tool/project_home.rs`

```rust
    fn os_user_of(&self, session_token: &str) -> Result<String, Status> {
        let github_user = (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
```

### Findings

All transport, credential and config pieces exist in `tddy-session-split` /
`tddy-sandbox-recipes` / `tddy-daemon-kernel`; the unsandboxed claude-cli launch of a main simply
never calls them. `tddy-agent-launch` already depends on `tddy-session-split`
(`tddy_session_split::service_util`) and holds a `SplitSessions` (`LaunchSessions.split_sessions`),
so no new dependency edge is needed; two `pub(crate)` items need widening (or a new `pub` wrapper).

## Exploration 3: tests and deferred-work records — 2026-10-24

**Agent**: parent Bash/Read
**Scope**: test seams; Step 2b scan

### Sequence

1. Read `tddy-agent-launch/src/claude_cli_spawn_steps/claude_cli_spawn_steps_tests.rs:1-140` + fn list
2. Grep fns in `tddy-session-lifecycle/tests/ensure_project_home_session_acceptance.rs`; read `1-80`, `365-395`
3. Read `tests/ensure_project_home_world/mod.rs`, `tests/project_home_support/mod.rs`
4. Grep `session_tokens|SessionTokens::` in lifecycle tests; read `sandboxed_codebase_placement_acceptance.rs:84-113,174-247`
5. Grep `pub fn|session_tokens|TEST_TOKEN` in `tddy-daemon-rpc` test_util and `tests/project_home_session_tools_acceptance.rs`
6. `ls packages/<pkg>/docs/code-issues/` for 7 packages; grep `Claimed by:`; grep touched file names
7. Grep `docs/dev/todo/` for `project.home|session_role|mcp-config|strict-mcp|agent.tool.socket|TDDY_REMOTE_DAEMON|home session|native tool`; read 6 bodies
8. `ls docs/dev/1-WIP/` — no WIP changeset touches these files

### Inspected files

#### `tests/project_home_support/mod.rs`

```rust
pub const VALID_TOKEN: &str = "valid-token";
…
    let user_resolver: UserResolver =
        Arc::new(move |token| (token == VALID_TOKEN).then(|| resolved_user.clone()));
    DaemonSessionHost::new(config.clone(), …, manager)   // no .with_session_tokens(..)
…
        "#!/bin/sh\necho \"ARGV: $@\"\necho \"ENVDUMP TDDY_SOCKET=[$TDDY_SOCKET]\"\nexec cat >/dev/null\n",
```

#### `tests/sandboxed_codebase_placement_acceptance.rs`

```rust
fn this_daemons_session_tokens() -> &'static SessionTokens { /* DaemonSigningKey::load_or_generate + SessionTokens::new(&key, Arc::new(StandaloneKeyDirectory)) */ }
fn a_caller_token() -> &'static str { /* this_daemons_session_tokens().signer().mint_access(&GitHubUser { login: current_os_user(), … }) */ }
```

#### `tddy-daemon-rpc/tests/project_home_session_tools_acceptance.rs`

Uses `TestDaemon` with a fixed `TEST_TOKEN` resolver — it cannot verify a minted token, so the
end-to-end "minted token authorizes `ExecuteTool`" check is done in lifecycle by verifying the token
under the daemon's own `SessionTokens` (the production resolver does exactly that).

### Code issues / TODOs found

| Record | Relevance |
|---|---|
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-resume-claude-cli-session-resume-claude-cli-session.md` | touched file, open, unclaimed |
| `docs/dev/todo/2026-10-24-sandboxed-main-needs-session-role.md` | sandboxed main; out of this scope |
| `docs/dev/todo/2026-10-23-project-home-git-push-remote-and-token.md` | becomes reachable once main has its tools |
| `docs/dev/todo/2026-10-17-desktop-cursor-hooks-reach-daemon.md` | same embedded-socket pattern, cursor only |
| `docs/dev/todo/2026-10-23-project-home-running-heuristic.md`, `…-auto-stop-untracked-processes.md`, `2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md` | same area, different concern |

No `Claimed by:` in any touched package. `tddy-session-tool-client` and `tddy-sandbox-recipes` have
no `docs/code-issues/` (not analyzed; read-only consumers here).

### Findings

A clean unit seam exists for the wiring function; the acceptance seam needs a signer and an env dump
in the shared project-home fixture.
