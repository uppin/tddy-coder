# 2026-08-16 — Models & Agents — adjacent findings

**Category:** Future enhancement
**Source:** models-and-assistants changeset, 2026-08-16

- **`ClaudeAcpBackend::default()` hardcodes `bunx claude-agent-acp`.**
  `packages/tddy-core/src/backend/acp.rs:360` spawns that command with no env var, no CLI flag and no
  coder-config override — unlike `CodexAcpBackend` (`codex_acp.rs:472`), which resolves through
  `TDDY_CODEX_ACP_CLI` → sibling of the `codex` binary → PATH, plus `--codex-acp-cli-path` and
  `codex_acp_cli_path`. An operator with `claude-agent-acp` installed anywhere other than where `bunx`
  finds it cannot use the backend at all. Give it the same resolution chain.
- **`acp.rs` and `codex_acp.rs` are ~80% duplicated.** Identical `acp::Client` impls, identical
  progress accumulators, identical `run_acp_worker` thread/`LocalSet` loops. `packages/tddy-acp`
  exists as the extraction target and says so in its `lib.rs` doc comment, but currently holds only
  `mapping.rs`. The models changeset adds a *third* ACP implementation there rather than unifying;
  the unification is still owed.
- **`ListAgentModels` has no peer forwarding.** `connection_service.rs:6247` — the
  `daemon_instance_id` field participates only in the cache key, so the session-creation model
  dropdown cannot enumerate a remote host's catalog even though `ListEligibleDaemons` +
  `forward_to_peer` exist and would supply it.
- **Two model catalogs will coexist.** `ListAgentModels` (per coding backend, via
  `tddy-tools list-models`) and the new `ModelRegistryService` (per provider, from SQLite) answer
  overlapping questions by different means. Unifying them — most likely by making the registry a
  provider *behind* `ListAgentModels` — is deferred.
- **Assistant definitions do not replicate between daemons.** Per-daemon SQLite was chosen so
  credentials stay on the host that uses them, but it means an assistant created on the laptop is
  invisible on the workstation. A common-room sync for the `assistant` table (not `provider`) is the
  natural follow-up.
- **Provider API keys are plaintext at rest.** `<tddy-data-dir>/models.db` (plus its `-wal`/`-shm`
  siblings) is created `0600`, but its **parent is the shared `tddy-data-dir`, mode 0755** — not the
  `0700` auth-storage directory `github_token_store.rs` uses. The data dir is deliberately readable
  by session processes running as other uids (`projects/`, per-user session bases), so `0700` there
  would break them; a `0600` file inside it is unreadable by those accounts, but the filename is
  visible. Moving to `<tddy-data-dir>/model-registry/models.db` would hide that too. Encryption is
  not used at all, so a host backup captures every key. The schema reserves a nullable
  `credential_ref` column for an env-var-reference mode that this changeset does not build.
