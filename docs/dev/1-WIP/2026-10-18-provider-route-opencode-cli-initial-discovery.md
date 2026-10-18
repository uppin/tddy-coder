# Initial Discovery: #provider-route 3/9 — 2026-10-18-provider-route-opencode-cli

**Changeset**: [2026-10-18-provider-route-opencode-cli.md](./2026-10-18-provider-route-opencode-cli.md)
**Date**: 2026-10-18
**Passes**: 3 (whole-stack discovery, copied in full as this node's record)

The three passes below were run once for the whole `#provider-route` stack before it was split
into nodes; this node's slice is named in its changeset's State A. Node-specific passes are
appended after them.

## Combined Conclusions

**Packages in play:** `tddy-service` (protos `accounts`, `session`, `catalog`), `tddy-accounts`,
`tddy-credentials`, `tddy-daemon` (accounts wiring), `tddy-agent-launch` (session start, claude-cli /
cursor-cli spawn, tool-session spawn, sandboxed starts, resume), `tddy-agent-backend` (`AnyBackend`,
claude/codex/acp backends, model catalog), `tddy-coder` (`run.rs` `create_backend`, flags),
`tddy-spawn` (tool-session env allowlist), `tddy-cli-sessions`, `tddy-pty`, `tddy-changeset`
(`.session.yaml`), `tddy-session-files`, `tddy-sandbox-recipes`, `tddy-sandbox`, `tddy-sandbox-runner`,
`tddy-daemon-sandbox`, `tddy-sandbox-app`, `tddy-daemon-kernel` (config), `tddy-tools`
(`list-models`), `tddy-web` (Accounts screen, Create-session pane).

**State A in one paragraph.** A session is selected by `session_type` (`tool` → headless
`tddy-coder --agent <id>` building an `AnyBackend`; `claude-cli` / `cursor-cli` → the vendor CLI in a
PTY, optionally jailed). No harness is ever pointed at a third-party endpoint: there is no
`ANTHROPIC_BASE_URL`, Codex `model_providers`, OpenCode or Muse anywhere. Harness credentials are
host/daemon-wide (Claude `.credentials.json` seeded into one jail home plus
`CLAUDE_CODE_OAUTH_TOKEN` from the daemon env; Cursor `~/.cursor` files; Codex its own login). The
only provider concept is the **model registry** (`models.proto` `ProviderKind {OLLAMA, OPENAI,
FIREWORKS, ANTHROPIC}`, plaintext key in `models.db`), which drives subagents, assistants and the
Models chat only, and assumes base URLs without `/v1`. The **account vault** (sealed, per-user,
open-string provider ids) holds GitHub accounts only and can only add accounts via device link.

**Decisions taken in the interview (2026-10-18):** provider accounts live in the **account vault**
(not the registry) — two provider concepts coexist, and a session on a vault account starts only
while the vault is open; Meta is both a **provider** and a **Muse Code TUI harness**; OpenCode is both
an **ACP tool-session agent** and a **TUI session type**; overrides at **session**, **agent
definition** and **daemon/user default** levels (session > agent def > default); providers: Meta,
Fireworks, Baseten, Modal, generic OpenAI-compatible, generic Anthropic-compatible; **Cursor is
excluded** (its CLI agent cannot be redirected) and shown as unsupported; new TUIs ship
**unsandboxed first**, sandboxed in a later node.

**Harness × wire protocol (capability matrix input):** Claude Code (headless, `claude-acp`,
`claude-cli`) needs **Anthropic Messages** (`ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN`,
`ANTHROPIC_MODEL`, `ANTHROPIC_DEFAULT_{OPUS,SONNET,HAIKU}_MODEL`, `CLAUDE_CODE_SUBAGENT_MODEL`) —
Meta `https://api.meta.ai`, Fireworks `https://api.fireworks.ai/inference`, generic
Anthropic-compatible. Codex / `codex-acp` need **OpenAI Responses** (`model_providers.<id>`
`base_url`, `env_key`, `wire_api="responses"`) — Meta `https://api.meta.ai/v1`. OpenCode accepts
Responses or Chat Completions via `opencode.json` providers. Muse Code takes only Meta
(`META_API_KEY`, secondary sources). Cursor: none.

**Env delivery per path (constraint on every node):** headless/ACP backends inherit `tddy-coder`'s
env; the daemon→`tddy-coder` spawn allows only five git-identity keys and the supervisor refuses
unlisted keys, so a key reaches a tool session as a **0600 file** (the `--agent-def-path`
precedent), not env; unsandboxed PTYs take free-form `spec.env`; jails `env_clear()` and receive
secrets only through `TDDY_SECRET_<NAME>` files (today only `CLAUDE_CODE_OAUTH_TOKEN`). Jail egress
has no host allowlist, so provider hosts are reachable.

**Backlog / code issues in the path (none claimed):** `tddy-coder/docs/code-issues/oversized-file-run.md`,
`tddy-agent-backend/docs/code-issues/oversized-file-backend-mod.md`,
`tddy-sandbox-runner/docs/code-issues/oversized-file-runner.md`,
`tddy-daemon-sandbox/docs/code-issues/oversized-file-sandbox-session.md`,
`tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md`,
`docs/dev/todo/2026-10-04-keyring-accounts-rename-lost-update.md`,
`docs/dev/todo/2026-10-08-cursor-cli-spawn-rs-in-tddy-agent-launch-is-563-production-lines.md`,
`docs/dev/todo/2026-10-08-drain-tddy-discovery-into-purpose-named-crates-and-remove-it.md`.
Packages with no `docs/code-issues/` (unanalyzed): `tddy-accounts`, `tddy-credentials`, `tddy-acp`,
`tddy-sandbox-recipes`, `tddy-spawn`.

**Open questions for wave 2:** whether the Meta Model API accepts the `x-api-key` header or only
bearer (Claude Code with `ANTHROPIC_AUTH_TOKEN` sends bearer — Meta docs say use that); Muse Code's
exact key variable (`META_API_KEY` per secondary sources) and `--model` flag; OpenCode's config
injection (`OPENCODE_CONFIG` path).

## Exploration 1: Accounts, credentials and provider registry — 2026-10-18

**Agent**: Explore subagent
**Scope**: tddy-accounts, tddy-credentials, tddy-credential-sync, tddy-model-registry, tddy-sandbox-recipes, tddy-agent-backend (codex), accounts/models/session/project protos, tddy-web accounts/models/create-session components, daemon YAMLs, docs/ft accounts/models docs.

### Sequence

1. `ls packages`, `ls docs/ft`, `find` names `*account*|*credential*|*vault*|*secret*|*token*|*askpass*|*auth*` — candidate crates/tests/docs.
2. Listed files of tddy-accounts, tddy-credentials, tddy-credential-sync, tddy-daemon-auth, tddy-askpass, tddy-model-registry, tddy-agent-backend, tddy-agent-launch and all `*.proto`.
3. Read `packages/tddy-service/proto/accounts.proto`.
4. Read `tddy-accounts/src/lib.rs`, `docs/accounts-service.md`.
5. Read `tddy-accounts/docs/account-linking.md`, `docs/github-identity-resolution.md`, `src/store.rs`, `src/linking.rs` (1–120).
6. Read `tddy-credentials/docs/credential-store.md`, `src/record.rs`.
7. Grep `ANTHROPIC_API_KEY|OPENAI_API_KEY|CURSOR_API_KEY|CODEX_HOME|CLAUDE_CONFIG_DIR|ANTHROPIC_BASE_URL|OPENAI_BASE_URL|MODEL_API_KEY|FIREWORKS|api\.meta\.ai` repo-wide.
8. Read `models.proto`.
9. Read `tddy-model-registry/docs/model-registry.md`.
10. Grep `store.rs` for `api_key|credential|CREATE TABLE|0o600`; `openai_compatible.rs` for auth style.
11. Read `tddy-web/src/components/models/AddProviderForm.tsx`, `store.rs` 895–960 (`validate_base_url`), `openai_compatible.rs` 1–60.
12. Grep `agent_def_for_spawn|agent-def-path`, credential usage.
13. Read `tddy-model-registry/src/assistant_def.rs`, `tddy-session-agents/src/spawn_agent_def.rs`, `tddy-discovery/src/agent_def.rs` 80–200.
14. Grep `def.api_key|.api_key` (non-test).
15. Read parts of `tddy-acp/src/provider_agent.rs`; grep `chat/completions`.
16. Grep `seed_cursor_credentials|seed_claude|.claude.json|claude_home|auth.json`.
17. Read `tddy-sandbox-recipes/src/claude_cli.rs`, `cursor_cli.rs` (functions/env), `tddy-daemon-kernel/src/config.rs` 800–815, 960–990.
18. Read `tddy-agent-backend/src/spawn_env.rs` head, `tddy-sandbox-runner/tests/secret_envs.rs` head; grep session RPCs.
19. Grep `TDDY_SECRET_|CLAUDE_CODE_OAUTH_TOKEN|resolve_secret_envs`; read `session.proto` 423–520 (`StartSessionRequest`).
20. Read `tddy-sandbox-recipes/src/plan.rs` 225–260.
21. `find` tddy-web `src`/`cypress` for account/provider/model/keyring/vault/connection/picker files.
22. Listed `components/models`, `CreateSession*`, stories.
23. Read `CreateSessionModelField.tsx`, `constants/claudeCliModels.ts`, grep `createSessionRequest.ts`, head of `ProjectAccountAssignment.tsx`.
24. Grep routes (`index.tsx`, `DaemonNavMenu.tsx`), list-models sources, config files.
25–26. Read daemon YAML sections (allowed_agents, github, claude_cli), `AllowedAgent`, `KeyringConfig`.
27. Read `codex.rs` 40–70, `docs/ft/daemon/codex-oauth-relay.md` head; grep `docs/ft/*/*.md` for account/provider/credential.
28–29. Pulled lines from six feature docs; grep `project.proto` for `SetProjectAccounts`.

### Grep / glob

| Tool | Pattern / glob | Path scope | Notable hits |
|------|----------------|------------|--------------|
| find | `*account*\|*credential*\|*vault*\|*secret*\|*token*\|*askpass*\|*auth*` | repo | tddy-accounts, tddy-credentials, tddy-credential-sync, tddy-daemon-auth, tddy-askpass; `accounts.proto`; `docs/ft/web/accounts-screen.md`; `docs/ft/daemon/{account-linking,credential-sync,session-auth,codex-oauth-relay}.md`; `tddy-session-split/src/agent_credentials.rs`; `tddy-sandbox-runner/tests/secret_envs.rs` |
| find | `*.proto` | packages | `tddy-service/proto/{accounts,auth,askpass,local_token,token,models,catalog,session,project,daemon_config}.proto` |
| grep | `ANTHROPIC_API_KEY\|OPENAI_API_KEY\|CURSOR_API_KEY\|…\|MODEL_API_KEY\|FIREWORKS\|api\.meta\.ai` | rs/ts/tsx/yaml/md/proto | no `ANTHROPIC_*`, `OPENAI_BASE_URL`, `MODEL_API_KEY`, `api.meta.ai` anywhere; `CURSOR_API_KEY` docs/comments only; `OPENAI_API_KEY` only `codex.rs:56`; `FIREWORKS` in `models.proto:72`, `AddProviderForm.tsx:15`, Cypress models tests |
| grep | `api_key\|credential\|CREATE TABLE\|0o600` | `tddy-model-registry/src/store.rs` | lines 28, 58–59, 190–215, 265–282, 1033–1041 |
| grep | `agent_def_for_spawn\|agent-def-path` | packages | `spawn_agent_def.rs`, `svc_start_session_core.rs:82`, `svc_resume_session.rs:136`, `svc_start_hosted_agent_clone.rs:170,182,231`, `tddy-coder/src/run.rs:229,455` |
| grep | `def.api_key\|.api_key` (non-test) | packages | `assistant_def.rs:111`, `acp_service.rs:332`, `provider_agent.rs:165`, `openai_compatible.rs:88`, `ollama.rs:67` |
| grep | `chat/completions` | tddy-acp, tddy-agent-backend | `provider_agent.rs:64`, `tests/provider_agent_acceptance.rs:35` |
| grep | `seed_cursor_credentials\|seed_claude\|claude_home\|auth.json` | packages | `sandbox_session.rs:808–842`, `config.rs:807–990`, `relaunch_jail_steps.rs:83`, `svc_start_sandboxed_claude_cli_session.rs:318`, `tddy-sandbox-app/src/main.rs:107,277,460` |
| grep | `TDDY_SECRET_\|CLAUDE_CODE_OAUTH_TOKEN\|resolve_secret_envs` | packages | `tddy-sandbox/src/builder.rs:504–510,733–744`, `materialize.rs:52`, `tddy-sandbox-darwin/src/spawn.rs:16`, `tddy-sandbox-recipes/src/plan.rs:243–247`, `tddy-sandbox-runner/src/runner.rs:992–1225` |
| grep | `rpc ListAgentModels\|ListAgents\|StartSession` | proto | `catalog.proto:18,23`, `session.proto:26,423` |
| grep | `"/accounts"\|"/models"` | tddy-web src | `index.tsx:54–55,208–210`, `DaemonNavMenu.tsx:108,128` |
| grep | `list-models\|claude_cli_models\|cursor_cli_models` | packages | `tddy-tools/src/list_models.rs`, `tddy-agent-backend/src/backend/cursor.rs:109–142` |
| grep | `group_secret\|KeyringConfig` | `config.rs` | 223, 233 |
| grep | `SetProjectAccounts` | `project.proto` | 31, 139–154 |

### Inspected files

#### `packages/tddy-service/proto/accounts.proto`
**Why**: account RPC surface.
`service AccountsService { ListAccounts; SetAccountLabel; RemoveAccount; BeginLinkAccount; PollLinkAccount }`. `AccountSummary { provider, account_id, label, subject, updated_at, has_secret, sync_status }` — no secret field; `provider` is an open string ("github", "cloudflare", "screen-sharing"). `ListAccountsResponse` carries `vault_locked`, `vault_uninitialized`, `optional SessionAccount session_account`. `BeginLinkAccountRequest.provider` "github today"; device-code shaped (`user_code`, `verification_uri`, `interval_seconds`). **No RPC adds an account by pasting an API key.**

#### `packages/tddy-accounts/src/lib.rs`, `docs/accounts-service.md`, `docs/account-linking.md`, `src/linking.rs`
`AccountStore` port (`list`, `set_label`, `remove`); production `SessionVaultAccountStore`. `resolve_account(assignments, provider, held)` → `Assigned | NotAssigned | UnknownOnThisHost | Ambiguous`, no fallback. Registered only when `auth_storage` configured. `AccountLinker { begin(provider), poll(link_id) }`, `LinkedAccountStore { held, put, session_account }`; dedup on `META_SUBJECT_ID`, id `github-<subject id>`; `LinkError::UnsupportedProvider`. Daemon wiring `tddy-daemon/src/account_linking.rs` (`GitHubAccountLinker`, `VaultLinkedAccountStore`).

#### `packages/tddy-accounts/docs/github-identity-resolution.md`
`acting_identity()` → `{account, token, git}`; the token is "asked for, per call, over a socket the session's host serves — never an environment variable, never a file".

#### `packages/tddy-credentials/docs/credential-store.md`, `src/record.rs`
`ProviderId`/`AccountId` string newtypes — "adding a provider is data, not a breaking change". `CredentialRecord { provider, account, label, secret: SecretString, metadata: BTreeMap<String,String>, updated_at, version }`, not serialisable; removal leaves a `Tombstone`. One file per subject `auth_storage/credentials-<hex subject>.vault`, 0600, atomic swap-rename. ChaCha20-Poly1305 under a random DK wrapped by Argon2id(passphrase) plus per-browser unlock slots; record ids are HMACs. "There is no daemon-held key." States `Open | Locked | Uninitialized`; idle TTL `github.open_vault_idle_ttl_seconds` (7 days). Per user, per daemon; propagation via `tddy-credential-sync` gated by `keyring.group_secret`.

#### `packages/tddy-service/proto/project.proto` (31–154)
`rpc SetProjectAccounts(repeated AccountAssignment)`; `(provider, account_id)` refs; one account per provider per project.

#### `packages/tddy-service/proto/models.proto`
`ModelRegistryService`: `ListProviders, CreateProvider, DeleteProvider, ListModels, RefreshProviderModels, Load/UnloadModel, List/Create/Update/DeleteAssistant, ListAssignableTools`.
```proto
enum ProviderKind { PROVIDER_KIND_UNSPECIFIED = 0; OLLAMA = 1; OPENAI = 2; FIREWORKS = 3; ANTHROPIC = 4; }
```
`ProviderEntry { provider_id, kind, label, base_url, has_credential, daemon_instance_id, enumeration_error }`; `CreateProviderRequest.api_key` "Inbound only"; duplicate base URL → `ALREADY_EXISTS`.

#### `packages/tddy-model-registry/docs/model-registry.md`, `src/store.rs`
`<tddy-data-dir>/models.db` (SQLite) 0600 (parent dir not 0700). `provider.credential TEXT` — "The api key at rest" — **plaintext**, not vault-sealed; reserved `credential_ref TEXT -- Reserved for the env-var-reference credential mode; unused today`. Reads unscoped; `delete_provider`, `update_assistant`, `delete_assistant`, `credential_for` owner-only. Per daemon, one OS user owner, no sync. `validate_base_url`: http(s), host required, no userinfo.

#### `packages/tddy-model-registry/src/openai_compatible.rs`
`CredentialStyle::{Bearer (OpenAI, Fireworks), AnthropicApiKey}`. Enumerates `format!("{}/v1/models", self.base_url)` — Meta's `https://api.meta.ai/v1` (and Fireworks `.../inference/v1`) would yield `/v1/v1/models`.

#### `packages/tddy-acp/src/provider_agent.rs`
"Provider root, e.g. `http://127.0.0.1:11434`; `/v1/chat/completions` is appended to it". Uses `tddy_discovery::openai::OpenAiClient`.

#### `packages/tddy-model-registry/src/assistant_def.rs`, `tddy-session-agents/src/spawn_agent_def.rs`, `tddy-discovery/src/agent_def.rs:107`
`assistant_to_agent_def` → `SpecializedAgentDef { model, base_url: provider.base_url, api_key: None, … }`; `registry_agent_def_with_credential` sets `def.api_key = store.credential_for(provider_id, caller)`. Registry def wins over `<tddyhome>/agents/*.yaml`; called from `svc_start_session_core.rs:82`, `svc_resume_session.rs:136`, `svc_start_hosted_agent_clone.rs:170`. `SpecializedAgentDef.base_url` required, `api_key: Option<String>` (redacted in Debug); child gets `--agent-def-path <file>` written 0600 + chowned (`tddy-coder/src/run.rs:229,455`).

#### `packages/tddy-web/src/components/models/AddProviderForm.tsx`
`PROVIDER_KIND_OPTIONS` = Ollama, OpenAI, Fireworks, Anthropic; fields kind, label, base URL (free text, no per-kind default), API key (password, never read back).

#### `packages/tddy-sandbox-recipes/src/claude_cli.rs`, `cursor_cli.rs`, `plan.rs:242–248`
`seed_claude_credentials(claude_home_dir)` copies host `~/.claude/.credentials.json` once; `claude_runner_env_overlay` sets only `CLAUDE_CODE_TMPDIR`, `CLAUDE_TMPDIR`. `seed_cursor_credentials` copies `cli-config.json`, `auth.json`, `mcp.json`; overlay `CURSOR_TMPDIR`, `CURSOR_SKIP_KEYCHAIN=1`, `CI=true`; "The caller must still supply `CURSOR_API_KEY`."
```rust
if recipe == SandboxRecipe::ClaudeCli {
    if let Some(token) = std::env::var("CLAUDE_CODE_OAUTH_TOKEN")...
        builder = builder.secret("CLAUDE_CODE_OAUTH_TOKEN", SecretSource::Value(token));
```
Only secret-injection path; reads the daemon's own env.

#### Secret delivery channel
`tddy-sandbox/src/builder.rs` (~504) writes each secret to a 0600 scratch file and passes `TDDY_SECRET_<NAME>=<path>`; `tddy-sandbox-runner/src/runner.rs:992–1225` `resolve_secret_envs` restores the real var for the inner child and deletes the file.

#### `packages/tddy-daemon-kernel/src/config.rs`
`claude_cli.claude_home_dir` daemon-wide persistent jail `$HOME` (default `$HOME/.tddy/sandbox-claude-home`, `TDDY_SANDBOX_CLAUDE_HOME`); `KeyringConfig` 223 (`group_secret`); `AllowedAgent { id, label }` 1185.

#### `packages/tddy-daemon-sandbox/src/sandbox_session.rs:808,842`
`prepare_persistent_claude_home` creates dir, seeds credentials and local install; Cursor seeded at 842. Callers `svc_start_sandboxed_claude_cli_session.rs:318`, `relaunch_jail_steps.rs:83`.

#### `packages/tddy-agent-backend/src/backend/codex.rs` (43–60)
Codex relies on `codex login` / on-disk auth, or `printenv OPENAI_API_KEY | codex login --with-api-key` (remediation hint only). OAuth via Codex OAuth relay.

#### `packages/tddy-service/proto/session.proto` `StartSessionRequest` (423+)
`agent` (4), `session_type` (7: "tool", "claude-cli", "cursor-cli"), `model` (8), `sandbox` (16), `managed_codebase` (17), subagent names. **No account/provider/base-URL field.**

#### tddy-web
`CreateSessionModelField.tsx` — model `<select>` from `ListAgentModels` (`useAgentModels`); `createSessionRequest.ts` sets `agent`/`model` per `sessionType`; `constants/claudeCliModels.ts` holds no list; `ProjectAccountAssignment.tsx` `assignmentRowsFor(assigned, vault)` one row per provider. Routes `#/accounts` (`AccountsAppPage`→`AccountsScreen`, `useLinkFlow.ts`) and `#/models` (`ModelsAppPage`→`ModelsScreen`) via `DaemonNavMenu.tsx`. Models components: `ProvidersPanel`, `AddProviderForm`, `ModelsTable`, `AssistantsPanel`, `CreateAssistantDialog`, `EditAssistantDialog`, `AssistantToolPicker`, `ModelChatDialog`, `ChatWorkspaceDialog`, `ModelsDialogShell`, `useModelRegistryFanOut`, `useAssistantToolSets`. `CredentialVaultPrompt.tsx`. Session creation: `CreateSessionPane`, `CreateSessionDialog`, `CreateSessionModelField`, `CreateSessionTypeToggle`, `CreateSessionCursorCliFields`. Stories: only `AccountsScreen.stories.tsx`. Cypress: `AccountsScreenAcceptance`, `AccountLinkingAcceptance`, `ProjectAccountsAcceptance`, `CredentialVaultPromptAcceptance`, `CreateSessionAgentPicker`, `CreateSessionModelSelectionAcceptance`, `cypress/component/models/*` (ProvidersPanelAcceptance covers FIREWORKS, ProviderRemoval, ModelsScreen, …, Assistant*). Page objects `accountsScreenPage.ts`, `modelsScreenPage.ts`, `credentialVaultPromptPage.ts`; RPC fakes `modelRegistryBackend.ts`, `linkingBackend.ts`, `projectServiceBackend.ts`, `sessionAgentRosterBackend.ts`.

#### Config files
`dev.daemon.yaml`: `github: stub: true`, TTLs, `users:` map, `allowed_agents:` claude, claude-acp, cursor, stub, codex, codex-acp; no `keyring:`, no models/providers section. `daemon.yaml.production`: `auth_storage`, commented `github:`, `users: []`. `desktop.yaml.production`: `tddy_data_dir`, `auth_storage`, `github.client_id`, `allowed_agents` claude, claude-acp, codex, codex-acp, cursor; "Discovered assistants are added to these rows at runtime."

#### Feature docs
`web/models-and-agents.md`: Provider = "an Ollama instance, an OpenAI account, Fireworks, Anthropic. Kind, base URL, optional credential. Belongs to exactly one daemon"; "added explicitly through the UI, never auto-detected"; credential reaches the child only via the 0600 `--agent-def-path` file. `web/accounts-screen.md`: grouped by provider, four vault outcomes. `daemon/account-linking.md`: "Linking never signs anyone in", "one person is one account". `daemon/credential-sync.md`: `keyring.group_secret` absent = propagate to nobody; LWW with tombstones. `daemon/github-identity.md`: token "asked for, never delivered". `web/tool-session-model-selection.md`: model lists from `tddy_core::backend` / `tddy-tools list-models`. `daemon/cursor-cli-session.md:123`: jail auth needs `CURSOR_API_KEY` or seeded `auth.json`.

### Findings

- **Two credential stores.** Account vault (sealed, per-user, sync-able, GitHub device-link only, open-string provider) vs model registry (plaintext SQLite `credential`, per-daemon, already has `FIREWORKS` and `ANTHROPIC` kinds, `CredentialStyle::{Bearer, AnthropicApiKey}`).
- **Neither store feeds Claude/Cursor/Codex harnesses.** Claude: host `.credentials.json` seeded into one daemon-wide jail home + `CLAUDE_CODE_OAUTH_TOKEN` from daemon env via `TDDY_SECRET_*`. Cursor: seeded `~/.cursor` files; `CURSOR_API_KEY` not set by code. Codex: own login / OAuth relay.
- **Registry providers drive only tddy's own OpenAI-compatible loop** (`SpecializedAgentDef` → `--agent-def-path` → `ProviderAcpAgent`/`OpenAiClient`), which appends `/v1/...` to a root — Meta `https://api.meta.ai/v1` would double `/v1`.
- **No per-session provider/account field** on `StartSessionRequest`; no `ANTHROPIC_BASE_URL`/`OPENAI_BASE_URL`/`MODEL_API_KEY` injection anywhere.
- **Reusable seams:** `TDDY_SECRET_*` file channel (per-session secret env into jails), 0600 agent-def file, open-string `ProviderId` in the vault, `ProviderKind` enum in `models.proto`.
- **Backlog in path:** `docs/dev/todo/2026-10-14-project-account-assignment-reaches-one-host.md`, `2026-10-07-keyring-link-github-add-account-ungated.md`, `2026-10-04-keyring-accounts-rename-lost-update.md`, `2026-08-16-the-daemon-s-secret-stores-still-truncate-in-place.md`.

## Exploration 2: Backend selection, spawn and env propagation — 2026-10-18

**Agent**: Explore subagent
**Scope**: tddy-agent-backend, tddy-acp, tddy-model-registry, tddy-discovery, tddy-coder (run.rs, config.rs), tddy-agent-launch, tddy-spawn, tddy-supervisor, tddy-cli-sessions, tddy-pty, sandbox chain (tddy-sandbox, -cgroups, -darwin, -recipes, -runner, tddy-daemon-sandbox, tddy-sandbox-app), daemon config/YAMLs, protos, tddy-web session-start + models.

### Sequence

1. `ls` crates in scope; `git log -3`.
2. `wc`, grep `enum .*(Backend|AgentKind|Harness|Provider)` in `*.rs`.
3. Read `tddy-agent-backend/src/lib.rs`, `spawn_env.rs`, `claude_argv.rs`, `backend/mod.rs` (whole), `backend/claude.rs` 1–680, `backend/model_catalog.rs`.
4. Grep `.env(|env_remove|env_clear|Command::new|"--model"|CODEX_HOME|OPENAI|CURSOR_|ANTHROPIC` in codex/cursor/acp/codex_acp/stub.
5. Read `codex.rs` 40–290, 430–490; `cursor.rs` 40–300; `acp.rs` 300–429; `codex_acp.rs` 410–539; `tddy-acp/src/provider_agent.rs` 1–160.
6. Read `tddy-model-registry/src/{lib,openai_compatible,provider_client,assistant_def}.rs`; grep `ProviderKind|client_for` in `service.rs`; list all `*.proto`; grep `models.proto`.
7. Grep agent/backend/model/session_type/provider in daemon_config, session, session_agents, types, sandbox protos; read `catalog.proto` 1–70, `daemon_config.proto` 50–110, `session.proto` 423–640.
8. Grep `"cursor-cli"|"claude-cli"` per file; read `svc_start_session_core.rs` 55–460, `tool_session_spawn.rs`.
9. Grep `fn create_backend|fn build_backend|AnyBackend::X(`; read `tddy-coder/src/run.rs` 2540–2740, 140–160, 245–265, 470–560, 585–625, 700–760, 1230–1330; `config.rs` 1–80.
10. Grep `"codex-acp"`, `"cursor"`, `"claude-acp"` across rs/ts/tsx/yaml/proto/json.
11. Read `tddy-sandbox-app/src/spawn.rs` 25–70, 530–560, 640–700.
12. Grep `build_sandbox_runner_env|subagent_env_overlay|env_clear()|ANTHROPIC_|CURSOR_API|allowlist`; read `tddy-daemon-sandbox/src/sandbox_session.rs` 440–740, 960–1060; `tddy-sandbox-cgroups/src/lib.rs` 60–320.
13. Read `tddy-sandbox/src/runner_env.rs`, `claude_cli.rs` 150–175, `cursor_cli.rs` 305–330, `builder.rs` 495–520, 700–790, `tddy-sandbox-recipes/src/plan.rs` 200–270.
14. Read `tddy-sandbox-runner/src/runner.rs` 985–1060, 1168–1260, 1429–1520, 2360–2440; `materialize_secrets`.
15. Grep `api.anthropic.com|allowed_hosts|egress_allow`; read `host_relay.rs` 770–815.
16. Read `claude_cli_spawn.rs` 170–316, `claude_cli_spawn_steps.rs` env_extra, `cli_session_manager/argv.rs`, `tddy-pty/src/runtime.rs` 225–300.
17. Read `tddy-spawn/src/spawner.rs` 425–470, 670–710, 830–870, 1130–1165; supervisor `spawn_broker.rs` 283–303, 1320–1370, `supervisor.proto` 70–110, `policy.rs` `allowed_env_keys`.
18. Read `jail_env_builders.rs` 1–60; sandboxed cursor svc 270–340; `tddy-daemon-kernel/src/config.rs` 400–470, 780–850.
19. Grep the 7 YAMLs for agent/claude/cursor/codex/model/provider/base_url/api_key.
20. Read `tddy-discovery/src/agent_def.rs` (`SpecializedAgentDef`), `openai.rs`, `backend.rs`; model-registry `store.rs` schema + `credential_for`, `service.rs` 120–145, 355–410, `acp_service.rs` 450–470.
21. Web: grep allowedAgents/ListAgentModels/sessionType/ProviderKind; read `CreateSessionTypeToggle.tsx`, `claudeCliModels.ts`, `useAgentModels.ts`, `createSessionRequest.ts` 1–120, `CreateSessionPane.tsx` agent/model lines; `AddProviderForm.tsx`, `ProvidersPanel.tsx` 25–45.
22. Read `tddy-daemon-rpc/src/catalog/agent_models.rs`, `ports.rs` 100–150; `tddy-tools/src/list_models.rs` 1–40.
23. Grep `opencode|model_providers|wire_api|ANTHROPIC_BASE_URL|ANTHROPIC_AUTH_TOKEN|ANTHROPIC_MODEL|CLAUDE_CODE_SUBAGENT_MODEL|api.meta.ai|OPENAI_BASE_URL|ANTHROPIC_DEFAULT` repo-wide — no hits.
24. Read `session_metadata.rs` 20–80, `svc_resume_session.rs`, `stream/mod.rs`, `agent_list_mapping.rs`, `context_files.rs` 60–100, `backend_invoke_task.rs` 110–130, `agent_session_runner.rs`, `tddy-sandbox-darwin/src/spawn.rs` 1–60.

### Grep / glob

| Tool | Pattern | Scope | Notable hits |
|------|---------|-------|--------------|
| grep | `enum .*(Backend\|AgentKind\|Harness\|Provider)` | `packages/**/*.rs` | `backend/mod.rs:34 AnyBackend`; `tddy-sandbox-app/src/spawn.rs:36 AgentKind`; `supervisor_client.rs:19 SpawnBackendChoice` (unrelated) |
| grep | `.env(\|env_clear\|"--model"\|OPENAI\|CURSOR_\|ANTHROPIC` | backend/*.rs | codex.rs 123/132/133/220–241; cursor.rs 111–113, 302–314; acp.rs 328; codex_acp.rs 426; codex.rs:56 (help text) |
| grep | `fn create_backend\|fn build_backend` | packages | `tddy-coder/src/run.rs:2592–2650`; `model_catalog.rs:99–115` |
| grep | `"codex-acp"` | rs/ts/tsx/yaml/proto/json | mod.rs 608/643/691/749/822; model_catalog 111; run.rs 2637/2675; backend_invoke_task 121/253; context_globs_acceptance 142/169/201; server_options_acceptance 62/232; acceptance_daemon 317; cypress responses 302; list_models.rs 18 |
| grep | `"cursor-cli"\|"claude-cli"` | non-test Rust | ~30 files (session_list_enrichment 13, session_metadata 6, telegram 8, context_files 4, svc_start_session_core 4, pty_relay 5, placement 3, …) |
| grep | `env_clear()` | sandbox | cgroups lib.rs 125/281; many in tests/action pipelines |
| grep | `TDDY_SECRET_` | packages | builder.rs 504/510; materialize.rs 52; darwin spawn.rs 16; runner.rs 992ff |
| grep | `allowed_env_keys` | rs/yaml/md | supervisor policy.rs:139, config.rs:198/393; dev.supervisor.yaml:77 |
| grep | `opencode\|model_providers\|wire_api\|ANTHROPIC_BASE_URL\|api.meta.ai\|…` | repo | **no hits** |
| grep | web `allowedAgents\|sessionType\|ProviderKind\|"cursor-cli"\|"claude-cli"` | tddy-web/src | CreateSessionTypeToggle, createSessionRequest, CreateSessionPane, SessionMainPane, SessionInspectorDrawer, AgentActivityOverlay, AddProviderForm, ProvidersPanel, claudeCliModels, toolShortcuts, useAgentModels |

### Inspected files

#### `packages/tddy-agent-backend/src/backend/mod.rs`
```rust
pub enum AnyBackend { Claude(ClaudeCodeBackend), ClaudeAcp(ClaudeAcpBackend), Cursor(CursorBackend),
    Codex(CodexBackend), CodexAcp(CodexAcpBackend), Stub(StubBackend) }
```
Five exhaustive match blocks (97–152). `CodingBackend` trait (842–876): `invoke`, `name`, `submit_channel`, `action_invoke_cache_eligible`, `list_models` (default `curated_models_for_agent(self.name())`), `context_globs`. `InvokeRequest` (419–454) has `model: Option<String>`, **no env/provider/base_url**; only extra-env channel `remote: Option<RemoteToolEnv>` (`TDDY_REMOTE_*`). String tables: `backend_selection_question` (513–546, six labels), `backend_from_label` (602–612, unknown → `("claude","opus")`), `default_model_for_agent` (639–647), `curated_models_for_agent` (689–697), `context_globs_for_agent` (731–752), `claude_cli_models` (761), `cursor_cli_models` (772–789), `preselected_index_for_agent` (816–826). Tests pin `len()==6`, label order, indexes (1055–1159).

#### `backend/model_catalog.rs`
`CLAUDE_CLI_AGENT="claude-cli"`, `CURSOR_CLI_AGENT="cursor-cli"`; `BackendCliPaths { cursor_cli_path, claude_acp_cli_path, codex_acp_cli_path }`; `build_backend(agent, paths)` (99–119) matches `claude|claude-acp|cursor|codex|codex-acp|stub`. Feeds `tddy-tools list-models`.

#### `claude_argv.rs`, `backend/claude.rs`
`build_claude_base_argv(binary, model, session_id, permission_mode, skip_permissions_flag, resume)` shared by sandboxed runner and `CliSessionManager`. Headless `build_claude_args` (206–276): `-p <prompt> --output-format stream-json --verbose`, `--model`, `--session-id/--resume`, `--append-system-prompt*`, `--allowedTools`, `--permission-prompt-tool`, `--mcp-config`. `invoke_sync` env (533–547) does not clear env:
```rust
cmd.env("PATH", super::path_with_exe_dir());
if let Some(ref p) = request.socket_path { cmd.env("TDDY_SOCKET", p); }
... TDDY_REPO_DIR, TDDY_SESSION_DIR, remote.env_pairs()
```

#### `backend/codex.rs`
`build_codex_exec_argv` (443–477): `exec [-C dir] -s read-only|workspace-write --json [resume] [-m model] [id] <prompt>` — **no `-c key=value`, no `--profile`**. Env (220–242): PATH, `TDDY_CODEX_OAUTH_OUT`, `BROWSER=<current exe>`, TDDY_*. No `CODEX_HOME`.

#### `backend/cursor.rs`
`DEFAULT_CLI_BINARY="agent"`; args `[--plan] [--resume id] -p <prompt> [--model m] --output-format stream-json --stream-partial-output --force --trust --approve-mcps`; `list_models_sync` parses `agent --list-models`. Writes `<wd>/.cursor/mcp.json`. **No base-URL/provider flag.**

#### `backend/acp.rs` (ClaudeAcpBackend), `backend/codex_acp.rs` (CodexAcpBackend)
Default `bunx claude-agent-acp`; `spawn_agent` (324–342) pipes stdio, sets no env (inherits). Codex ACP binary from `TDDY_CODEX_ACP_CLI`/`codex-acp`; `spawn_codex_acp_agent` (422–443) sets no env. `list_models` from ACP `new_session().models`.

#### `stream/mod.rs`
Parsers: claude (NDJSON, also cursor via `process_ndjson_stream`), codex, cursor.

#### `tddy-acp/src/provider_agent.rs`
```rust
pub struct ProviderAgentConfig { pub base_url: String /* `/v1/chat/completions` is appended */, pub model: String,
    pub api_key: Option<String>, pub system_prompt: Option<String>, pub connect_timeout, pub request_timeout }
```
ACP ↔ OpenAI-compatible HTTP via `tddy_discovery::openai::OpenAiClient`; `MAX_TOOL_ROUNDS_PER_TURN=10`.

#### `tddy-model-registry/src/*`
`openai_compatible.rs`: `GET {base_url}/v1/models`; `CredentialStyle { Bearer, AnthropicApiKey }` (x-api-key + anthropic-version). `service.rs` `client_for` (366–409) matches `Ollama | Openai|Fireworks | Anthropic | Unspecified`, no catch-all; `create_provider` (122–145) refuses Unspecified. `store.rs` schema (1033–1047) `provider(provider_id, kind INTEGER, label, base_url UNIQUE, credential TEXT, credential_ref TEXT, enumeration_error, owner)`. `acp_service.rs` 453–465 `reject_a_kind_this_chat_cannot_speak` refuses Anthropic for chat (TODO: `/v1/messages` planned).

#### `tddy-discovery/src/agent_def.rs` (107–150), `openai.rs`, `backend.rs`
`SpecializedAgentDef { name, label, model, base_url (required), api_key: Option (redacted), system_prompt, system_prompt_path, usage_notes, tools, max_turns, replaces: Vec<String> }`. `OpenAiClient` posts `{base_url}/v1/chat/completions`. `SpecializedAgentBackend::from_def` implements `CodingBackend`.

#### `tddy-coder/src/run.rs`, `config.rs`
`--agent` free string (601–611); `--agent-def-path` (613–619) 0600 JSON; `-m/--model`; path flags `--cursor-agent-path`, `--codex-cli-path`, `--codex-acp-cli-path`. `create_backend` (2592–2665): def by name wins → `SpecializedAgentBackend`, else match built-ins, unknown errors "Known agents: …". `BUILTIN_BACKEND_AGENT_IDS = ["claude","claude-acp","cursor","codex","codex-acp","stub"]` (2670), test ~3049. `verify_tddy_tools_available` skips stub/claude-acp (147–149); `agent_activity_source_for` (255). coder-config YAML `deny_unknown_fields`: `agent, model, cursor_agent_path, codex_cli_path, codex_acp_cli_path`.

#### `tddy-agent-launch/src/svc_start_session_core.rs`, `tool_session_spawn.rs`
Agent resolution (74–95): `agent_roster.agent_def_for_spawn` else must be in `allowed_agents`. claude-cli/cursor-cli require model (145–154). Dispatch on `session_type`: `workspace` (229), `claude-cli` (352; sandboxed or not), `cursor-cli` (391), else tool session (440). `ToolSpawnPlan { agent, agent_def_json, model, git_environment, … }`.

#### `tddy-spawn/src/spawner.rs`
```rust
pub const SESSION_GIT_ENVIRONMENT_KEYS: [&str; 5] = ["GIT_AUTHOR_NAME","GIT_AUTHOR_EMAIL","GIT_COMMITTER_NAME","GIT_COMMITTER_EMAIL", GITHUB_SOURCE_ENV];
```
`require_commit_identity_only` refuses other keys ("Never a credential."). Direct spawn (1150–1158) `.env("HOME").env("PATH").env_remove("GITHUB_TOKEN").env_remove("GH_TOKEN").envs(env)` — inherits daemon env. Argv `--agent`, `--agent-def-path <session_dir>/agent-def.json` (0600, chowned), `--model`.

#### `tddy-supervisor/src/spawn_broker.rs`, `policy.rs:139`
`EnvironmentBase::Minimal` (PATH, HOME/USER/LOGNAME, locale) + request env; unlisted key in `spawn_policy.allowed_env_keys` refuses the spawn. `dev.supervisor.yaml:77`: `RUST_LOG`, `GIT_*`, `TDDY_GITHUB_SOURCE`.

#### `claude_cli_spawn.rs`, `claude_cli_spawn_steps.rs`, `cli_session_manager/argv.rs`, `tddy-pty/src/runtime.rs` (268–284)
`env_extra = git_environment + TDDY_SOCKET, PATH + semantic-index env`. PTY:
```rust
cmd.env("TERM", "xterm-256color"); cmd.env("COLORTERM","truecolor");
for token_variable in GITHUB_TOKEN_VARIABLES { cmd.env_remove(token_variable); }
for (key, value) in &spec.env { cmd.env(key, value); }
```
Inherits daemon env minus GitHub tokens; free-form `spec.env`. `build_cursor_argv`: `[binary, --resume?, --model?, prompt?]`.

#### Sandbox chain
`tddy-sandbox-cgroups/src/lib.rs` 123–126, 279–282: `cmd.env_clear(); cmd.envs(&plan.spec.env)`. `tddy-sandbox-darwin/src/spawn.rs` 14–25: `sandbox-exec -f <profile> /usr/bin/env -i KEY=VAL … <cmd>` (non-secret values on argv). `tddy-sandbox/src/runner_env.rs`: `process_jail_env` (HOME, TMPDIR, TERM, PATH) + `scratch_runner_env` (TDDY_SANDBOX_*, RUST_LOG, `TDDY_EGRESS_PROBE_*` passthrough only). Overlays: claude `CLAUDE_CODE_TMPDIR`, `CLAUDE_TMPDIR`; cursor `CURSOR_TMPDIR`, `CURSOR_SKIP_KEYCHAIN=1`, `CI=true`. `sandbox_session.rs` `build_sandbox_runner_env` (471), `build_sandboxed_cursor_runner_env` (503). `plan.rs` 238–252 only secret: `CLAUDE_CODE_OAUTH_TOKEN` from daemon env for `SandboxRecipe::ClaudeCli`. `builder.rs` 504–512 + `materialize.rs` 53–73: `TDDY_SECRET_<NAME>=<scratch>/.secrets/<NAME>` 0600. `runner.rs` `run_claude_pty_thread` (1168ff) sets TERM, `TDDY_EGRESS_SHIM`, `HTTPS_PROXY`/`HTTP_PROXY` (+lowercase) to the shim, then `resolve_secret_envs` (992–1017) sets `NAME=value` on the agent child and unlinks; `CommandBuilder` inherits runner env. Dispatch `args.agent_kind == "cursor"` → `spawn_cursor_pty`, else `spawn_claude_pty` (2384). `host_relay.rs` 792–815 `spawn_tunnel` connects to any CONNECT host — **no egress host allowlist**. `jail_env_builders.rs`: `specialized_subagent_env` → `TDDY_SUBAGENT`, `TDDY_SUBAGENTS_JSON` (full defs incl. `api_key`). `svc_start_sandboxed_claude_cli_session.rs:442–472` `jail_runner_env`. `tddy-sandbox-app/src/spawn.rs` `enum AgentKind { Claude, Cursor }` (36–51, 543, 650–668).

#### Config, proto, web
`config.rs`: `allowed_agents: Vec<AllowedAgent{id,label}>` (416, 1185); `ClaudeCliConfig{binary_path, tddy_tools_path, daemon_url, claude_home_dir}` (787–806); `CursorCliConfig{binary_path="agent", …, cursor_home_dir}` (809–826). YAML `allowed_agents` = claude, claude-acp, cursor, codex, codex-acp (dev.daemon 137–149, dev.desktop 98–110, desktop.yaml.production 113–123). `catalog.proto`: `ListAgents` → `AgentInfo{id,label}`; `ListAgentModels{agent, daemon_instance_id}` → `ModelInfo{id,label}` + `default_model`. `session.proto` `StartSessionRequest`: `agent=4`, `session_type=7`, `model=8`, `claude_args=25`, `sandbox=16`, `managed_codebase=17`, `specialized_agents=18`; 19–23 reserved; next free 41. `session_metadata.rs` `.session.yaml` (`deny_unknown_fields`) persists `session_type, model, agent, cursor_chat_id, sandbox, recipe, agents`; resume dispatches on `session_type` (`svc_resume_session.rs` 54/69/90). `agent_models.rs`: `ListAgentModels` runs `tddy-tools list-models --agent <id>`, cached 60s. Web: `createSessionRequest.ts:7` `type SessionType = "tool" | "claude-cli" | "cursor-cli"`; `CreateSessionTypeToggle.tsx` three buttons; `CreateSessionPane.tsx` `modelAgentKey` (355–361), `canCreate` (432–437); `AddProviderForm.tsx:12–17` `PROVIDER_KIND_OPTIONS`.

#### Other agent-name consumers
`agent_session_runner.rs:27` `backend_is_agent_driven` = claude|claude-acp; `backend_invoke_task.rs:121,253` codex|codex-acp resume; `tddy-tui/src/mouse_map.rs:671`; `install` 71–123, 960–980 codex-acp bundling.

### Findings

- **Two selectors, no provider.** `session_type` (tool / claude-cli / cursor-cli / workspace — PTY TUI sessions vs headless) and `agent` (tool sessions only → `tddy-coder --agent` → `AnyBackend`). No wire enum; agent ids are strings matched in many places.
- **Provider concept lives only in the model registry + `SpecializedAgentDef`** (subagents, assistants, Models chat). No harness is ever pointed at a third-party endpoint; zero hits for `ANTHROPIC_BASE_URL`, `opencode`, `model_providers`, `api.meta.ai`.
- **Env reaches children differently per path:** headless/ACP backends inherit; tool-session spawn allows only 5 git-identity keys (and supervisor `allowed_env_keys` refuses others); PTY inherits + free `spec.env`; jails `env_clear` and build a fixed map, with secrets via `TDDY_SECRET_*` files (only `CLAUDE_CODE_OAUTH_TOKEN` today). Credentials to tool sessions travel as the 0600 agent-def file, the established pattern.
- **Codex needs argv `-c model_providers.*` or a per-session `CODEX_HOME/config.toml`.** Cursor CLI has no base-URL flag.
- **Interactive (TUI) harness sessions exist only for claude-cli and cursor-cli**; a new PTY session type (opencode-cli, muse-cli) touches ~30 files (dispatch, resume, metadata, telegram pickers, pty relay, sandbox runner `--agent-kind`, recipes, sandbox-app, web toggle).
- **New headless backend touch list:** `AnyBackend` + 5 matches, 7 string tables + pinning tests, `model_catalog::build_backend`, stream parser, `run.rs::create_backend` + `BUILTIN_BACKEND_AGENT_IDS`, `config.rs`, `list_models.rs`, `allowed_agents` YAMLs, `install`, agent-name special cases.
- **Egress:** no host allowlist, so `api.meta.ai` and Fireworks hosts are reachable from jails.

## Exploration 3: Vault access at session start, claude-cli env, accounts UI — 2026-10-18

**Agent**: parent Grep/Read
**Scope**: the exact surfaces the draft-PR contracts will name.

### Sequence

1. `ls packages/tddy-credentials/src packages/tddy-accounts/src packages/tddy-accounts/tests` — crate layout.
2. Grep `^pub |pub fn` in `tddy-credentials/src/{lib,vault,sessions}.rs` — vault API.
3. Read `packages/tddy-accounts/src/vault_store.rs` 1–90; grep `fn |subject` in `service.rs` — how the accounts service reaches a subject's vault.
4. Grep `use_open(` (non-test) — who reads an open vault on behalf of a session.
5. Grep `acting_identity` (non-test); grep `github_user|session_token|subject` in `svc_start_session_core.rs`.
6. Read `packages/tddy-agent-launch/src/session_acting_identity.rs` 1–160.
7. Read `packages/tddy-changeset/src/session_metadata.rs` 20–80.
8. Read `packages/tddy-agent-launch/src/claude_cli_spawn_steps.rs` 200–250.
9. `ls packages/tddy-web/src/components/accounts`; grep `Add account|beginLinkAccount`.

### Grep / glob

| Tool | Pattern | Scope | Notable hits |
|------|---------|-------|--------------|
| grep | `^pub \|pub fn` | tddy-credentials | `vault.rs:163 CredentialStore`, `:252 SessionVault {put,get,list,remove,entries}`, `sessions.rs:77 SessionVaults`, `:196 state`, `:229 retain`, `:391 use_open` |
| grep | `use_open(` non-test | packages | `tddy-daemon-auth/src/github_pr_credentials.rs:79`, `tddy-daemon/src/account_linking.rs:175`, `tddy-daemon/src/credential_sync.rs:310`, `tddy-screen-sharing/src/session_vault_target_store.rs:48`, `tddy-accounts/src/vault_store.rs:42` |
| grep | `acting_identity` non-test | packages | `tddy-agent-launch/src/session_acting_identity.rs:75,108,146,265`, `tddy-accounts/src/identity.rs:140` |
| grep | `github_user\|session_token` | `svc_start_session_core.rs` | 67 `user_resolver(&req.session_token)`, 82 `agent_def_for_spawn(agent, &github_user)`, 253, 304 |

### Inspected files

#### `packages/tddy-credentials/src/vault.rs`, `sessions.rs`
`SessionVault::{put(CredentialRecord), get(&ProviderId,&AccountId), list(Option<&ProviderId>), remove, entries}`; `SessionVaults::{state(subject) -> VaultState, retain(subject, record), use_open(subject) -> Option<Arc<SessionVault>>, …}`.

#### `packages/tddy-accounts/src/vault_store.rs`
```rust
pub type SessionSubjectResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;
pub struct SessionVaultAccountStore { vaults: Arc<SessionVaults>, subject_of: SessionSubjectResolver }
fn vault_for(&self, session_token: &str) -> Result<(String, Arc<SessionVault>), AccountsError> { … state → Locked/Uninitialized refusals … use_open … }
```
`set_label` carries `TODO(keyring)` lost-update note (`docs/dev/todo/2026-10-04-keyring-accounts-rename-lost-update.md`).

#### `packages/tddy-agent-launch/src/session_acting_identity.rs`
`SessionAccountAccess { vaults: Option<Arc<SessionVaults>>, subject_of, session_token, github_cli }` — "What a session start may read to learn who it acts as". `acting_identity(accounts)` reads the session owner's vault at start; refusals `SessionIdentityRefusal::{NoVaultsConfigured, Vault(AccountsError), Identity}` with user-facing Display ("the signed-in person's credential vault is locked on this daemon"). Module rule: the GitHub token is never put in env or a file — a stricter rule than a model-provider key needs (a harness CLI can only read its key from env/config).

#### `packages/tddy-changeset/src/session_metadata.rs`
`.session.yaml` fields include `session_type`, `model`, `cursor_chat_id`, `hook_token`, `sandbox`, `agent`, `recipe`, `agents`; struct is `deny_unknown_fields`, so a new optional field is additive with `#[serde(default, skip_serializing_if = "Option::is_none")]`.

#### `packages/tddy-agent-launch/src/claude_cli_spawn_steps.rs` 200–250
`let mut env_extra = git_environment;` then managed-workflow env and semantic-index env are pushed; returned as `(managed, append_system_prompt_file, env_extra)` → PTY `spec.env`.

#### `packages/tddy-web/src/components/accounts/`
`AccountsAppPage.tsx`, `AccountsScreen.tsx` (Add account button at :228 → `useLinkFlow.ts` `beginLinkAccount({ sessionToken, provider })`), `AccountsScreen.stories.tsx`, `rpcReason.ts`.

### Findings

- Session start already resolves the owner (`user_resolver(session_token)`) and has a vault reader (`SessionAccountAccess`); a provider-account key can be read the same way at start, with the same refusal vocabulary (locked / uninitialized / not found) — no fallback.
- The accounts service's per-subject vault access (`vault_for`) is the place a `ConnectProviderAccount` write goes (`SessionVault::put` a `CredentialRecord` with `provider = "meta" | "fireworks" | …`, `secret = api key`, `metadata = base URLs per wire protocol + preset id`).
- `.session.yaml` takes an additive optional field for the chosen account id.
- The Accounts screen's add flow is device-link only; an API-key form is a new branch beside it.
