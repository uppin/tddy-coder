# Initial Discovery: Local gh as a project's GitHub source

**Changeset**: [2026-10-14-local-gh-github-source.md](./2026-10-14-local-gh-github-source.md)
**Date**: 2026-10-14
**Passes**: 3

## Combined Conclusions

1. **A project's GitHub identity today has exactly one source: the signed-in person's vault.**
   `tddy_accounts::acting_identity(assignments, provider, held)` (`tddy-accounts/src/identity.rs:140-187`)
   returns `ActingIdentity { account, token, git }` from one vault record; the git email is
   `{subject_id}+{login}@users.noreply.github.com`. There is no non-vault constructor, and
   `session_git_environment(&ActingIdentity)` (`tddy-daemon-livekit/src/session_git.rs:23-41`) only
   takes that type.
2. **The session edge is `SessionAccountAccess::session_identity`**
   (`tddy-agent-launch/src/session_acting_identity.rs:165-215`), which yields `SessionIdentity {
   git_environment, github_credential }`. It is called from every start/resume path: co-located
   claude-cli (`claude_cli_spawn.rs:83-92`, `svc_resume_claude_cli_session.rs:84-92`), plain cursor
   (`cursor_cli_spawn.rs:313-318`), jailed (`jail_launch_steps.rs:70-108`, `relaunch_jail_steps.rs:36-51`,
   `svc_start_sandboxed_cursor_cli_session.rs:234-246`) and tool sessions (`tool_session_spawn.rs:91-135`,
   `svc_resume_session.rs:70-83`). The assignment is read from **this host's** `projects.yaml`
   (`svc_session_identity_wiring.rs:6-56`). A refusal logs a `warn` and the session still starts
   (`session_acting_identity.rs:196-206`) — the consented exception Local gh reuses.
3. **The token is asked for per call** over `github-token` (`tddy-toolcall/src/toolcall/github_token_client.rs:16-48`,
   host side `tddy-host-service/src/host_session_service.rs:212-229`, listener
   `tddy-toolcall/src/toolcall/listener.rs:472-495`). One client serves `tddy-tools`
   (`github_credential.rs:20-44`, `server.rs:1050-1064`) and the workflow
   (`tddy-workflow-recipes/src/orchestrate_pr_stack/actions.rs:104-107,163-173`). Daemon-side
   `RepointPlannedPr` resolves through `project_github_token` (`tddy-daemon-rpc/src/pr_stack/ports.rs:538-562`).
4. **The project → account assignment does NOT propagate between hosts.** Vault *records* do
   (credential sync, `docs/ft/daemon/credential-sync.md`), but `SetProjectAccounts` writes one host
   (`coordinate_handlers.rs:487-528`; the web sends to `group.hosts[0]`, `ProjectsScreen.tsx:269,301-306`),
   and a session's identity reads the target host's own row (`tddy-session-split/src/service_util.rs:43`).
   Docs claim otherwise (`project-concept.md:28`, `tddy-projects/docs/project-service.md:41-46`,
   `projects-screen-multi-host.md:67-70`). Developer decision: the Local gh fact is **carried on
   `StartSession`** by the forwarding host (`start_request_checks.rs:31-45`) rather than written to every host.
5. **`GH_TOKEN` / `GITHUB_TOKEN` are scrubbed only on the `tddy-spawn` path** (`spawner.rs:1149-1157`).
   PTY spawns (`tddy-pty/src/runtime.rs:261-270`), the host-side shell relay
   (`tddy-tool-engine/src/contained_shell.rs:48-85`) and `spawn_as_user` (`tddy-daemon-kernel/src/spawn_as_user.rs:186-193`)
   inherit them. `gh` prefers `GH_TOKEN` over `hosts.yml`, so for Local gh this is a correctness hole.
6. **A jailed agent cannot run `gh` in the jail** (`env -i`, `HOME` = persistent claude home or
   scratch, `PATH=/usr/bin:/bin:/usr/sbin:/sbin`, `(deny network*)` except loopback/unix sockets,
   keychain hangs with `errSecMissingEntitlement`). Its shell commands run **host-side in the daemon
   process as the daemon's OS user** (`DaemonToolHandler::execute` → `run_contained`). On a
   multi-user daemon that `gh` is the daemon user's.
7. **The host already probes `gh auth status` per OS user** (`tddy-host-service/src/host_tooling.rs`,
   `GetHostTooling` in `host.proto:25,168-221`), returning the login but **no user id** and no
   per-hostname answer. `HostRowTooling` is **not mounted** anywhere and nothing calls `GetHostTooling`
   (`docs/dev/todo/2026-09-06-no-code-path-issues-gethosttooling.md`).
8. **The spawn wire admits only four env keys** (`SESSION_GIT_ENVIRONMENT_KEYS`,
   `tddy-spawn/src/spawner.rs:433-456`, refused by `require_commit_identity_only`); the supervisor
   policy must allow-list each key it forwards.
9. **Telegram sessions get no identity** (`workflow_spawn.rs:311-349`, notice `354-364`) because no
   vault can be opened without a session token. Local gh needs no vault.
10. **`ProjectData` has no `Default`**; ~41 files build it by struct literal (64 `accounts: Vec::new(),`).
11. **Pushes never use the vault token** (`tddy-git/src/remote.rs:143-170` uses host git credentials);
    `github-identity.md:20` claiming "pushes" is inaccurate.
12. **PR status still uses a login-keyed token** (`tddy-daemon-rpc/src/pr_stack/pr_status.rs:56`,
    `retained_github_token`) — out of this change's path.

Conflicting WIP changesets: none in `docs/dev/1-WIP/` touch project accounts, session identity or
`github-token`.

## Exploration 1: feature docs and record scan — 2026-10-14

**Agent**: parent Read/Bash
**Scope**: product docs for GitHub identity, account linking, project assignment; Step 2b scan.

### Sequence

1. `cat .agents/skills/planning/references/planning-phase.md`, `.agents/skills/deferred-work/references/planning-cross-check.md` — process.
2. `git grep -il "github account\|GithubAccount\|github_account\|GitHubAccount" -- '*.rs' '*.ts' '*.tsx' '*.proto' '*.md' '*.yaml'` — map the area.
3. Read `docs/ft/daemon/github-identity.md`, `docs/ft/daemon/account-linking.md`, `docs/ft/daemon/project-concept.md`.
4. Read `packages/tddy-accounts/docs/github-identity-resolution.md`, `docs/ft/web/accounts-screen.md`.
5. `git grep -n "ProjectAccount\|message AccountRef\|AccountAssignment" -- '*.proto'`; `git grep -n "gh auth\|\"gh\"" -- '*.rs'`.
6. Read `packages/tddy-service/proto/project.proto:30-60`, `packages/tddy-host-service/src/host_tooling.rs:1-80`.
7. `ls packages/<pkg>/docs/code-issues/` for 13 packages; `grep -l 'Claimed by:'`; `grep -rl -iE 'gh auth|gh cli|github cli|local gh|project.?account|acting.?identity|session identity|github-token|host_tooling' docs/dev/todo/`.
8. Read TODOs `2026-09-06-no-code-path-issues-gethosttooling.md`, `2026-10-04-keyring-assignments-grew-project-coordinate-handlers.md`, `2026-10-08-keyring-github-identity-oversized-files.md`, `2026-09-18-desktop-install-configures-no-identity.md`; code issues `tddy-daemon-rpc/docs/code-issues/oversized-file-project-coordinate-handlers.md`, `tddy-workflow-recipes/docs/code-issues/squatting-pr-stack-data-model.md`.

### Grep / glob

| Tool | Pattern / glob | Path scope | Notable hits |
|------|----------------|------------|--------------|
| git grep | `github account\|GithubAccount\|...` | repo | 60+ files across tddy-accounts, tddy-agent-launch, tddy-daemon-rpc, tddy-toolcall, tddy-tools, tddy-web |
| git grep | `AccountAssignment` | `*.proto` | `tddy-service/proto/project.proto:39,121,140` |
| git grep | `gh auth\|"gh"` | `*.rs` | only `tddy-host-service/src/host_tooling.rs` and `tddy-daemon-kernel/src/spawn_as_user.rs:227` |
| grep | `Claimed by:` | code-issues of 13 pkgs | only `tddy-workflow-recipes/.../squatting-pr-stack-data-model.md` — **unclaimed** ("#496 … releases the remainder") |

### Inspected files

#### `packages/tddy-service/proto/project.proto:39-44`

```proto
message AccountAssignment {
  // Provider this assignment is for ("github", "cloudflare", ...).
  string provider = 1;
  // Account at that provider, as `accounts.AccountSummary.account_id` reports it.
  string account_id = 2;
}
```

#### `packages/tddy-host-service/src/host_tooling.rs:7-9,56-63`

```rust
//! Both are **per-OS-user**: `git config --global` reads `$HOME/.gitconfig` and `gh auth status`
//! reads `$HOME/.config/gh/hosts.yml`. A probe run as the daemon's own user would report a
//! different machine's answer, so both go through [`tddy_daemon_kernel::spawn_as_user::start_output_as_user`].
pub struct GithubCliStatus {
    pub outcome: ProbeOutcome,
    pub installed: bool,
    pub authenticated: bool,
    /// The login `gh` is authenticated as — the **host's**, not the tddy session's.
    pub login: Option<String>,
}
```

#### `docs/ft/daemon/github-identity.md` (excerpt)

> The process environment is not a credential. `GITHUB_TOKEN` and `GH_TOKEN` are never read, in the
> daemon or in a session, and exporting them rescues nothing.
> … **The session still starts**, without account commit identity, and the refusal is recorded in the
> daemon log; commits then carry the checkout's own `user.name` and `user.email`.

#### `packages/tddy-daemon-rpc/docs/code-issues/oversized-file-project-coordinate-handlers.md`

> **Metrics:** **650 production lines** … budget 500 · **1.3× over** · **Status:** Open — **unclaimed**
> Seam B: branch operations: `set_project_default_branch_at_project_coordinate`, … `set_project_accounts_at_project_coordinate` (~285 with it)

### Findings

- The vault is the only identity source; the product rule is "token and identity from one resolution".
- `gh` is already probed per OS user, for display only.
- No claimed code issue sits on the files this change edits. `coordinate_handlers.rs` is over budget
  and unclaimed; `listener.rs` (766 lines) is over budget per the 2026-10-08 TODO.

## Exploration 2: identity, token, sandbox and web paths — 2026-10-14

**Agent**: Explore subagent ("very thorough"); no `.codegraph/` index exists.
**Scope**: project model and storage; session identity at start/resume; token request path; host gh
probe; sandbox; web Projects screen; Telegram; docs; test fixtures.

### Sequence

1. `ls -a .codegraph` (absent), `ls packages`
2. `grep -n "accounts\|Account\|SetProjectAccounts" packages/tddy-service/proto/project.proto`
3. `grep -rln -e SetProjectAccounts -e set_project_accounts -e setProjectAccounts packages docs tddy-coder`
4. `grep -n "accounts\|Account" packages/tddy-projects/src/*.rs`
5. `grep -n "accounts\|Accounts\|fn \|default_branch" packages/tddy-daemon-rpc/src/project/coordinate_handlers.rs`
6. `grep -n "set_project_accounts\|SetProjectAccounts\|set_project_default_branch" packages/tddy-daemon-livekit/src/livekit_peer_discovery.rs packages/tddy-daemon-rpc/src/test_util.rs`
7. `grep -n "SetProjectAccounts\|setProjectAccounts\|accounts" packages/tddy-web/src/components/projects/*.tsx`
8. `grep -n -i "account\|every host\|fan" docs/ft/web/projects-screen-multi-host.md`
9. `grep -rn "session_identity\|SessionAccountAccess" packages docs --include=*.rs --include=*.md`
10. `grep -rn "session_acting_identity" packages/tddy-session-lifecycle/src packages/tddy-agent-launch/src/lib.rs`
11. `grep -rn "git_environment\|github_credential_handler\|github_credential" packages/tddy-agent-launch/src`
12. `grep -rn "git_environment" packages/tddy-spawn/src`
13. `grep -n "SESSION_GIT_ENVIRONMENT_KEYS\|GITHUB_TOKEN\|GH_TOKEN\|env_remove" packages/tddy-spawn/src/*.rs`
14. `grep -rn 'env_remove("GITHUB_TOKEN")\|"GH_TOKEN"\|"GITHUB_TOKEN"' packages --include=*.rs`
15. `grep -rn "no_crate_still_resolves_a_github_token_from_the_process_environment" . --include=*.rs --include=*.md`
16. `grep -rn "GithubCredentialHandler\|GITHUB_TOKEN_METHOD\|\"github-token\"" packages --include=*.rs`
17. `grep -n "github_credential\|GithubToken\|no credential handler\|github-token" packages/tddy-toolcall/src/toolcall/*.rs`
18. `grep -n "github_credential\|github-token\|request_github_token\|GithubToken" packages/tddy-tools/src/server.rs`
19. `grep -rn "request_github_token\|asking_the_session_host\|github_token" packages/tddy-workflow-recipes/src`
20. `grep -rn "project_github_token" packages --include=*.rs`
21. `grep -n "fn token_for_repoint" -A 25 packages/tddy-daemon-rpc/src/pr_stack/*.rs`; `grep -rln "RealGithubPrApi::with_token\|asking_the_session_host\|without_credential" packages`
22. `grep -rn "fn retained_github_token" -A 30 packages`
23. `grep -n "fn \|GithubCliStatus\|struct \|enum \|start_output_as_user\|find_program_on_spawn_child_path" packages/tddy-host-service/src/host_tooling.rs`
24. `grep -n "pub fn start_output_as_user\|pub fn find_program_on_spawn_child_path\|pub fn merge_spawn_child_path" packages/tddy-daemon-kernel/src/*.rs`
25. `grep -rln "GithubCli\|github_cli\|githubCli" packages/tddy-service/proto packages/tddy-host-service/src packages/tddy-web/src packages/tddy-web/cypress packages/tddy-daemon-rpc/src`
26. `grep -rn "HostRowTooling\|getHostTooling\|GetHostTooling" packages/tddy-web/src packages/tddy-web/cypress`
27. `find packages -name "*.sb"`; `grep -n "HOME\|keychain\|mach-lookup\|PATH\|fn " packages/tddy-sandbox-darwin/src/profile.rs`
28. `grep -rn '"HOME"\|"PATH"\|claude_home\|CLAUDE_CONFIG_DIR' packages/tddy-sandbox/src packages/tddy-sandbox-recipes/src packages/tddy-daemon-sandbox/src packages/tddy-sandbox-runner/src packages/tddy-agent-launch/src`
29. `grep -rn "session_env" packages --include=*.rs`; `grep -rn "fn dial_and_bridge"`; `grep -rn "Command::new\|\.envs(\|env_clear\|env_remove" packages/tddy-tool-engine/src`
30. `grep -rn "env_clear\|env_remove" packages/tddy-pty/src packages/tddy-terminal-rpc/src packages/tddy-cli-sessions/src`
31. `grep -rn -i "keychain\|securityd\|\.config/gh\|hosts.yml" packages docs`
32. `grep -rn -i "daemon's own user\|runs as the daemon\|as the daemon user" docs/ft packages/...`
33. `grep -n "projectAccountRow\|projectAccountSelect\|projectAccountUnavailable" packages/tddy-web/cypress/support/testIds.ts`; `grep -rn "export function anInMemoryRpcBackend" packages`
34. `grep -n "identity\|git_environment\|GIT_\|account\|host_session_socket\|SpawnOptions\|fn " packages/tddy-telegram-control/src/telegram_session_control/workflow_spawn.rs`
35. `grep -rln "ProjectData {" packages`; `grep -rn "accounts: Vec::new()," packages | wc -l`
36. `grep -rn "credential.helper\|x-access-token\|GIT_ASKPASS" packages --include=*.rs`; `grep -rn '"push"' packages/tddy-git/src`

### Grep / glob

| Tool | Pattern / glob | Path scope | Notable hits |
|------|----------------|------------|--------------|
| grep | `SetProjectAccounts` | packages, docs | proto, `tddy-projects`, `tddy-daemon-rpc/src/project/coordinate_handlers.rs:465-583`, `livekit_peer_discovery.rs:1522-1542`, `ProjectsAppPage.tsx:91-104` |
| grep | `session_identity` | packages | `session_acting_identity.rs:165-215`, `svc_session_identity_wiring.rs`, `launch_ports.rs:63-76`, 7 call sites |
| grep | `env_remove("GH_TOKEN")` | packages | only `tddy-spawn/src/spawner.rs:1154-1155` |
| grep | `GITHUB_TOKEN_METHOD` | packages | `tddy-toolcall/src/toolcall/mod.rs:409`, `listener.rs:472-495`, `host_session_service.rs:212-229`, `conversation_spawn_relay.rs:185-212` |
| grep | `GetHostTooling` | tddy-web src | **no caller** in `src/`; only Cypress specs |
| grep | `ProjectData {` | packages | 41 files; 64 `accounts: Vec::new(),` |
| grep | `GIT_ASKPASS\|credential.helper` | packages | none — pushes use host git credentials |

### Inspected files

#### `packages/tddy-projects/src/project_storage.rs:14-52, 264-296`

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectData {
    pub project_id: String,
    pub name: String,
    pub git_url: String,
    pub main_repo_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_branch_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_name: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub host_repo_paths: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accounts: Vec<AccountAssignment>,
}

pub fn set_project_accounts(projects_dir: &Path, project_id: &str, accounts: &[AccountAssignment]) -> anyhow::Result<()> {
    let mut seen = std::collections::HashSet::new();
    for a in accounts {
        if !seen.insert(a.provider.as_str()) {
            anyhow::bail!("more than one account assigned for provider {}; a project uses one account per provider", a.provider);
        }
    }
    let mut projects = read_projects(projects_dir)?;
    let project = projects.iter_mut().find(|p| p.project_id == project_id)
        .ok_or_else(|| anyhow::anyhow!("unknown project: {}", project_id))?;
    project.accounts = accounts.to_vec();
    write_projects(projects_dir, &projects)
}
```

#### `packages/tddy-service/proto/project.proto:103-147`

```proto
  repeated AccountAssignment accounts = 8;   // ProjectEntry, fields 1-7 = id, name, git_url, main_repo_path, daemon_instance_id, main_branch_ref, default_remote
message SetProjectAccountsRequest {
  string session_token = 1;
  string project_id = 2;
  repeated AccountAssignment accounts = 3;
  string daemon_instance_id = 4;
}
message SetProjectAccountsResponse { ProjectEntry project = 1; }
```

#### `packages/tddy-daemon-rpc/src/project/coordinate_handlers.rs:465-583` (shape)

Authenticate → `os_user_for_github` → `classify_peer_route(local, requested, eligible)` → forward via
`forward_set_project_accounts_via_livekit` **or** refuse a repeated provider (`InvalidArgument`), check
the project exists (`NotFound`), `project_storage::set_project_accounts`, return
`entries::project_entry_from(&stored, local_id, default_remote)` (`entries.rs:55-77`, the single mapper).

#### `packages/tddy-accounts/src/identity.rs:38-187`

```rust
pub const PROVIDER_GITHUB: &str = "github";
pub struct GitIdentity { pub name: String, pub email: String }
pub struct ActingIdentity { pub account: AccountId, pub token: String, pub git: GitIdentity }
pub enum IdentityError {
    NotAssigned { provider: ProviderId },
    UnknownOnThisHost { provider: ProviderId, account: AccountId },
    Ambiguous { provider: ProviderId },
    Unusable { provider: ProviderId, account: AccountId, missing: &'static str },
}
// acting_identity(): … Ok(ActingIdentity { account, token: record.secret.expose().to_string(),
//     git: GitIdentity { name: login.clone(), email: format!("{subject_id}+{login}@users.noreply.github.com") } })
```

#### `packages/tddy-agent-launch/src/session_acting_identity.rs:24-59, 96-106, 165-215`

```rust
pub enum SessionIdentityRefusal { NoVaultsConfigured, Vault(AccountsError), Identity(IdentityError) }

pub fn project_github_token(vaults: Option<Arc<SessionVaults>>, subject_of: SessionSubjectResolver,
    session_token: &str, accounts: &[AccountAssignment]) -> Result<String, String> { … }

pub struct SessionIdentity {
    pub git_environment: Vec<(String, String)>,
    pub github_credential: Option<SharedGithubCredential>,
}
impl SessionAccountAccess {
    pub fn session_identity(&self, session_id: &str, accounts: Option<&[AccountAssignment]>) -> SessionIdentity {
        let Some(accounts) = accounts else { return SessionIdentity { git_environment: Vec::new(), github_credential: None }; };
        let (git_environment, pinned) = match self.acting_identity(accounts) {
            Ok(acting) => (session_git_environment(&acting), PinnedAccount::Acting(acting.account)),
            Err(refusal) => {
                log::warn!(target: "tddy_daemon::connection_service",
                    "session {session_id} starts without an account identity: {refusal}");
                (Vec::new(), PinnedAccount::Refused(refusal.to_string()))
            }
        };
        SessionIdentity { git_environment,
            github_credential: Some(Arc::new(SessionGithubCredential { access: self.clone(), pinned })) }
    }
}
```

#### `packages/tddy-session-lifecycle/src/connection_service/svc_session_identity_wiring.rs:6-56`

```rust
pub(crate) fn project_account_assignments(&self, os_user: &str, session_id: &str, project_id: &str) -> Option<Vec<AccountAssignment>> {
    if project_id.trim().is_empty() { return None; }
    match super::service_util::find_registered_project(&self.tddy_data_dir, os_user, project_id) {
        Ok((_, project)) => Some(project.accounts),
        Err(status) => { log::warn!(…"session {session_id} has no account identity: its project could not be read: {}", status.message()); None }
    }
}
pub(crate) fn session_identity(&self, os_user: &str, session_id: &str, project_id: &str, session_token: &str) -> SessionIdentity {
    let accounts = self.project_account_assignments(os_user, session_id, project_id);
    self.session_account_access(session_token).session_identity(session_id, accounts.as_deref())
}
```

Exposed to agent-launch via `LaunchHost` (`launch_ports.rs:63-76`), implemented in `svc_agent_host_ports.rs:127-138`.

#### `packages/tddy-daemon-livekit/src/session_git.rs:23-41`

```rust
#[must_use]
pub fn session_git_environment(acting: &ActingIdentity) -> Vec<(String, String)> {
    let GitIdentity { name, email } = &acting.git;
    [("GIT_AUTHOR_NAME", name), ("GIT_AUTHOR_EMAIL", email), ("GIT_COMMITTER_NAME", name), ("GIT_COMMITTER_EMAIL", email)]
        .into_iter().map(|(key, value)| (key.to_string(), value.clone())).collect()
}
```

#### Carriers of the commit pairs

- co-located claude-cli start: `claude_cli_spawn.rs:83-92` → `claude_cli_spawn_steps.rs:168-172,197-216` (`env_extra`) → `tddy-pty/src/runtime.rs:261-270` (no env scrub).
- co-located resume: `svc_resume_claude_cli_session.rs:84-92`.
- plain cursor-cli: `cursor_cli_spawn.rs:313-318` (pairs only; no token handler).
- jailed: `jail_launch_steps.rs:70-108`, `relaunch_jail_steps.rs:36-51`, `svc_start_sandboxed_cursor_cli_session.rs:234-246` → `DaemonToolHandler.session_env` (`tddy-daemon-sandbox/src/sandbox_session.rs:235-300`).
- tool session: `tool_session_spawn.rs:91-135`, `svc_resume_session.rs:70-83,147-187` → `ToolSpawnPlan.git_environment` (`tool_spawn_plan.rs:47-50`) → `SpawnOptions.git_environment` (`tddy-spawn/src/spawner.rs:425-430`).

```rust
// tddy-spawn/src/spawner.rs:433-456
pub const SESSION_GIT_ENVIRONMENT_KEYS: [&str; 4] = ["GIT_AUTHOR_NAME", "GIT_AUTHOR_EMAIL", "GIT_COMMITTER_NAME", "GIT_COMMITTER_EMAIL"];
fn require_commit_identity_only(git_environment: &[(String, String)]) -> anyhow::Result<()> { … anyhow::bail!(
    "a session spawn's environment may carry only the commit identity variables ({}); refusing `{key}`", …) }
// spawner.rs:1149-1157
        .env("HOME", &home_dir)
        .env("PATH", &child_path)
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .envs(env.iter().map(|(key, value)| (key, value)))
```

Supervisor path: `tddy-spawn/src/supervisor_spawn.rs:71-84`; the policy's `allowed_env_keys` must list each key.

#### `packages/tddy-toolcall/src/toolcall/github_token_client.rs:16-48`

```rust
pub async fn request_github_token_from_session() -> Result<String, String> {
    let socket = std::env::var_os("TDDY_SOCKET").map(std::path::PathBuf::from);
    request_github_token(socket.as_deref()).await
}
pub async fn request_github_token(socket: Option<&Path>) -> Result<String, String> {
    let socket = socket.ok_or_else(|| "TDDY_SOCKET is not set, so there is no session host to ask for this project's GitHub \
         account; GitHub operations only work in a managed session".to_string())?;
    let response = dispatch_toolcall(socket, serde_json::json!({ "type": "github-token" })).await?;
    match response["status"].as_str() { Some("ok") => …, Some("error") => …, _ => Err("…unrecognised answer to github-token".to_string()) }
}
```

#### `packages/tddy-toolcall/src/toolcall/listener.rs:54-64, 472-495`

```rust
#[async_trait]
pub trait GithubCredentialHandler: Send + Sync { async fn github_token(&self) -> Result<String, String>; }
// handle_github_token: no handler => Error "no GitHub account can be resolved for this session: its listener has no credential handler"
```

#### `packages/tddy-tools/src/github_credential.rs:20-44`, `server.rs:1050-1064`

```rust
pub(crate) fn github_token_from(socket: Option<&Path>) -> Result<String, String> { … block_on(tddy_core::toolcall::request_github_token(socket)) … }
pub(crate) fn github_api_for(repo: String, socket: Option<&Path>) -> Result<RealGithubPrApi, String> {
    Ok(RealGithubPrApi::with_token(repo, github_token_from(socket)?))
}
// server.rs: github_token_for_agent() = github_token_from(permission_relay_socket_path().as_deref()); real_gh() = github_api_for(repo_slug()?, …)
```

#### `packages/tddy-host-service/src/host_session_service.rs:212-229`

```rust
async fn handle_github_token(&self, payload: &[u8]) -> Result<Vec<u8>, Status> {
    let session = self.session_for(payload)?;
    let handler = session.github_credential_handler.ok_or_else(|| Status::failed_precondition(
        "no GitHub account can be resolved for this session: its host has no credential handler for it"))?;
    let token = handler.github_token().await.map_err(Status::failed_precondition)?;
    serde_json::to_vec(&serde_json::json!({ "token": token })) …
}
```

#### `packages/tddy-workflow-recipes/src/orchestrate_pr_stack/actions.rs:104-107,163-173`; `merge_pr/github.rs:23-36`; `github_pr_tools.rs:24-30`

```rust
let token = tddy_core::toolcall::request_github_token_from_session().await?;
let gh = super::github::RealGithubPrApi::with_token(&repo, token);
// RepointTask asks only if repoint_reaches_github(...), else RealGithubPrApi::without_credential(&repo)
pub fn github_pr_tools_available(context: &Context) -> bool {
    context.get_sync::<bool>(GITHUB_PR_TOOLS_AVAILABLE_KEY).unwrap_or(false)
}
```

`tddy-coder/src/tool_host_wiring.rs:28-53`: `github_pr_tools_available()` is `self.github_credential.is_some()`, seeding `presenter.set_github_pr_tools_available` at `run.rs:1828`.

#### `packages/tddy-daemon-rpc/src/pr_stack/ports.rs:538-562`, `guards.rs:34-42`

```rust
let token = token_for_repoint(node_owns_branch, || {
    let project_id = tddy_core::read_session_metadata(&session_dir)…?.project_id;
    let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))…?;
    let project = project_storage::find_project(&projects_dir, project_id.trim())…?;
    project_github_token(self.credential_vaults.clone(), Arc::clone(&self.user_resolver), &req.session_token, &project.accounts)
})?;
pub(crate) fn token_for_repoint(node_owns_branch: bool, resolve: impl FnOnce() -> Result<String, String>) -> Result<Option<String>, Status> {
    if !node_owns_branch { return Ok(None); }
    resolve().map(Some).map_err(Status::failed_precondition)
}
```

#### `packages/tddy-daemon-auth/tests/login_time_token_store_is_retired.rs:76-91`

```rust
fn no_crate_still_resolves_a_github_token_from_the_process_environment() {
    let source = format!("{}{}", production_source_of("tddy-github"), production_source_of("tddy-workflow-recipes"));
    assert!(!source.contains("github_token_from_env"), …);
}
```

#### `packages/tddy-host-service/src/host_tooling.rs:96-154, 228-260`

```rust
pub fn classify_gh_auth_status(exit_code: Option<i32>, output: &str) -> GithubCliStatus { … }
fn parse_gh_login(output: &str) -> Option<&str> {   // `✓ Logged in to github.com account octocat (keyring)`
    output.lines().find_map(|line| {
        let after_host = line.split_once("Logged in to ")?.1;
        let after_account = after_host.split_once(" account ")?.1;
        after_account.split_whitespace().next()
    })
}
let gh_program = tddy_daemon_kernel::spawn_as_user::find_program_on_spawn_child_path("gh");
let github_cli = gh_program.as_deref().map(|gh| start_probe(os_user, gh, &["auth", "status"]));
```

`PROBE_TIMEOUT = 5s`; `StartedProbe::collect` kills on timeout. Wire: `host.proto:213-221` `HostGithubCli { outcome, installed, authenticated, login, failure_reason }`; handler `service.rs:463-509` (peer-routed, then `spawn_blocking(probe.probe(os_user))`).

#### `packages/tddy-daemon-kernel/src/spawn_as_user.rs:77-79, 186-193`

```rust
pub fn find_program_on_spawn_child_path(program: &str) -> Option<PathBuf> { find_program_on_path(program, &merge_spawn_child_path(None)) }
    cmd.args(args).current_dir(&home_dir).env("HOME", &home_dir).env("PATH", merge_spawn_child_path(None))
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());   // no GH_TOKEN scrub
```

#### Sandbox

- `tddy-sandbox-darwin/src/spawn.rs:17-30`: `sandbox-exec -f <profile> /usr/bin/env -i K=V… <cmd>` — the jail starts clean.
- `tddy-sandbox/src/runner_env.rs:7-14`: `HOME`=scratch, `PATH=/usr/bin:/bin:/usr/sbin:/sbin`.
- `tddy-daemon-kernel/src/config.rs:933-956` `resolve_claude_home_dir` — "deliberately separate from the daemon user's real `~/.claude`".
- `tddy-sandbox-darwin/src/profile.rs:226-243`: `(deny network*)` + unix sockets + declared loopback ports.
- `tddy-daemon-sandbox/src/sandbox_session.rs:497-499`: keychain from the jail hangs (`errSecMissingEntitlement`).
- `tddy-tool-engine/src/contained_shell.rs:48-85`: `run_contained` = `sh -c` in the daemon process, inherited env + `session_env`.

#### Web: `packages/tddy-web/src/components/projects/`

- `ProjectsAppPage.tsx:21-149`: `useProjectsRpc` (5 s poll), `setProjectAccounts` over the selected daemon's client, `useAssignableAccounts`.
- `ProjectsScreen.tsx:11-46` props (`accounts`, `onSetProjectAccounts({ projectId, accounts, daemonInstanceId })`), `groupByProject` (68-88), `assignmentRowsFor` (104-120), `withAssignment` (123-134), `ProjectCard` (245-464; `primaryHost = group.hosts[0]` at 269), account rows 346-382 with test ids `project-account-row-${projectId}-${provider}`, `project-account-select-…`, `project-account-unavailable-…`.
- `HostRowTooling.tsx:112-134` `githubCliCell` ("Not installed" / "Not authenticated" / login); not mounted.

```tsx
// cypress/component/ProjectAccountsAcceptance.cy.tsx:36-123
function aProject(overrides: Partial<ProjectEntry>): ProjectEntry { return { projectId: PROJECT_ID, name: "alpha", gitUrl: "https://example.com/alpha.git", … accounts: [], ...overrides } as ProjectEntry; }
function aProjectsBackend(projects: ProjectEntry[]): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .onUnary(ProjectService.method.listProjects, () => ({ projects: state }))
    .onUnary(ProjectService.method.listProjectBranches, () => ({ branches: [], defaultRemote: "origin" }))
    .onUnary(ProjectService.method.setProjectAccounts, (req) => { … });
}
```

Page object `cypress/support/pages/projectsScreenPage.ts:132-175` (`accountRow`, `accountSelect`, `accountRowProviders`, `assignedAccountId`, `assignAccount`); test ids `cypress/support/testIds.ts:1401-1413`; `ProjectsScreenAcceptance.cy.tsx` passes `accounts={[]}`.

#### Telegram: `packages/tddy-telegram-control/src/telegram_session_control/workflow_spawn.rs:311-364`

```rust
        host_session_socket: None,
        // No commit identity: the project's account is resolved from the signed-in owner's credential vault …
        git_environment: &[],
pub(super) fn account_identity_unavailable_notice(project: &ProjectData) -> Option<String> {
    let account = project.accounts.first()?;
    Some(format!("This project acts as the {} account `{}`, but a session started from Telegram has no signed-in web session … Start the session from the web dashboard to act as the project's account.", …))
}
```

Called from `pickers.rs:703`, `session_start.rs:546,710`.

#### Test fixtures to follow

- `tddy-accounts/tests/project_resolved_identity_acceptance.rs`: `github()`, `a_github_account(account, subject_id, login, token)`, `a_project_assigning(account)`, `a_vault_holding_ada_and_grace()`; Given/When/Then comments; `assert_eq!` on whole values.
- `tddy-agent-launch/src/session_acting_identity_tests.rs` (651 lines): `a_daemon_where_ada_unlocked`, `vaults_where_ada_unlocked`, `access_with`, `credential_handler_over`, `pair(pairs, key)`.
- `tddy-session-lifecycle/tests/tool_session_git_identity.rs`: `a_tool_that_dumps_its_environment`, `register_project(data_dir, repo, accounts)`, `ChildEnvironment::{commit_identity, secret_bearing_lines}`, `World::{start_a_tool_session, resume_a_tool_session, child_environment}`.
- `tddy-session-lifecycle/tests/tool_session_host_socket.rs`: `World` with `.start`, `.resume`, `.the_only_child()`, `token_over(socket, session_id)`.
- `tddy-toolcall/tests/github_token_request_acceptance.rs`: `FixedCredential`, `a_session_host_answering(outcome)`.
- `tddy-projects/tests/project_accounts_storage.rs`: `a_project(id)`, `an_assignment`, `stored_accounts`.
- `tddy-daemon-rpc/tests/set_project_accounts_acceptance.rs`: `test_config`, `test_service`, `given_a_registered_project`, `a_set_request`, `stored_accounts`.

### Findings

Recorded as Combined Conclusions 1-3, 5-12. Also: `ProjectsScreen.tsx:2` imports `ProjectEntry` from
`session_pb` though it is exported from `project_pb` (verify during green).

## Exploration 3: propagation and StartSession forwarding — 2026-10-14

**Agent**: parent Bash/Read
**Scope**: the developer asked whether GitHub access propagates to remote daemons, so the Local gh
fact could ride the same channel.

### Sequence

1. `ls docs/dev/changesets | grep -i keyring`; `grep -rln -i "propagat" docs/ft/daemon packages/tddy-accounts/docs packages/tddy-credentials/docs packages/tddy-projects/docs`
2. Read `docs/ft/daemon/credential-sync.md`; `grep -n -i "assign\|project" docs/dev/changesets/2026-10-04-keyring-assignments.md`
3. `grep -rn "accounts" packages/tddy-daemon-livekit/src/livekit_peer_discovery.rs`; read `tddy-session-split/src/service_util.rs:43-55`
4. `grep -n "message StartSessionRequest" -A40 packages/tddy-service/proto/*.proto`; `sed -n 444,480p packages/tddy-service/proto/session.proto`
5. `git grep -n "forward_start_session" -- '*.rs'`
6. `grep -n network packages/tddy-sandbox-darwin/src/profile.rs`

### Inspected files

#### `docs/ft/daemon/credential-sync.md`

> **Travels:** a record's provider, account, label, metadata, secret and version — sealed so only the intended recipient can open it.
> Sync fires when a peer joins the common room … Propagation goes only to peers sharing a deployment-configured secret (`keyring.group_secret`).

#### `packages/tddy-session-split/src/service_util.rs:43-55`

```rust
pub fn find_registered_project(tddy_data_dir: &Path, os_user: &str, project_id: &str)
    -> Result<(std::path::PathBuf, tddy_projects::project_storage::ProjectData), Status> {
    let projects_dir = tddy_daemon_kernel::user_paths::projects_path_for_user(os_user, Some(tddy_data_dir)) …
```

#### `packages/tddy-service/proto/session.proto:404-480`

`StartSessionRequest` fields 1-29 (session_token, tool_path, project_id, agent, daemon_instance_id,
recipe, session_type, model, branch intents, …, `sandbox = 16`, `managed_codebase = 17`, …,
`repo_path = 24`, … `attachments = 29`, …). No GitHub/account field.

#### `packages/tddy-agent-launch/src/start_request_checks.rs:31-45`, `svc_start_session_core.rs:116`

`forward_start_session(&req, peer_instance_id)` → `forward_start_session_via_livekit(...)` forwards the
request as received.

### Findings

- Vault **records** propagate (credential sync); project **assignments** do not, and a forwarded start
  carries none. A session's identity on a peer host is read from that host's own `projects.yaml`.
- `StartSessionRequest` is the natural carrier: the forwarding host fills it from its own row.
- The jail network is loopback/unix-socket only, so nothing inside a jail can reach GitHub directly.
