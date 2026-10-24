# Initial Discovery: agent terminal tabs (#warp-agent 2/2)

**Changeset**: [2026-10-24-agent-terminal-tabs.md](./2026-10-24-agent-terminal-tabs.md) — Exploration 1–3 copied from the `#warp-agent` whole-work discovery; Exploration 3 is this node's map. Node 1 (`feature/warp-agent/warp-cli-session`, #717) holds its own copy.
**Date**: 2026-10-24
**Passes**: 3

## Combined Conclusions

### Warp facts (external, checked 2026-10-24)

- **`warp`** is the Warp Agent CLI — an interactive TUI, **separate from Warp.app** (Homebrew cask
  `warp-agent-cli`, or the install script). Not on this Mac's `PATH`; Warp.app bundles only the older
  `oz` (`/Applications/Warp.app/Contents/Resources/bin/oz`, `Oz v0.2026.10.14.08.27.stable_01`).
- Documented flags: `--resume CONVERSATION_TOKEN`, `--auto-approve`, `--api-key`,
  `--set-provider-api-key`, `--clear-provider-api-key`, `--help`, `--version`. Env: `WARP_API_KEY`,
  `WARP_TUI_DISABLE_AUTOUPDATE`. **No model flag, no first-prompt flag** — `/model` and
  `/agent [prompt]` are in-session slash commands.
- **The resume token is printed only on exit**: "On exit, the CLI prints a resume command in the
  form `warp --resume CONVERSATION_TOKEN`." Where conversations are stored locally is undocumented.
- Context: `AGENTS.md` or `WARP.md` (repo root down to cwd), global `~/.agents/AGENTS.md`, skills in
  `.agents/skills/`. Settings: `~/.warp_cli/settings.toml` (macOS),
  `~/.config/warp-terminal/cli/settings.toml` (Linux).
- `oz agent run -p … --output-format ndjson --model … -C <cwd> --mcp … --conversation <cloud id>` is
  a headless runner — **not** used by this stack (the operator chose the interactive TUI).

### Developer decisions (interview, 2026-10-24)

| Question | Answer |
|---|---|
| Primary agent | new `warp-cli` session type running the interactive `warp` TUI |
| "Additional agent on the same session" | a **second agent terminal tab** in the session's tab strip, same worktree, beside bash tabs |
| Which harnesses an agent tab can run | **any registered CLI harness** (claude-cli, cursor-cli, opencode-cli, muse-cli, warp-cli) |
| Auth | **native Warp login only** for now — no `warp` provider-account preset (deferred) |
| Resume | in scope — scrape `warp --resume <TOKEN>` from the PTY capture tail on every exit, persist it; fresh start when none recorded |
| Out of scope | sandbox (jailed Warp), model selection |
| Shape | a stack (`/plan-pr-stack`), two nodes once the provider preset was dropped |

### What a new CLI harness touches (Exploration 2)

One `CliHarness` descriptor in its own module (precedent `cli_harness_muse.rs`, #667) gives start and
resume dispatch, binary resolution (`TDDY_WARP_CLI` → `cli_harnesses.warp-cli.binary_path` →
`warp`), context agent, `ListAgentModels` catalogue, branch prefix (`BranchIntentPolicy::cli_harness`)
and the PTY spawn. On top:

- `Harness::WarpCli` in `provider_route.rs` (`id`, `ALL`, `product`, `route_supported` → refuse every
  preset, `route_body`) and the compile-forced arm in `tddy-agent-launch/src/harness_profile_start.rs:28`.
- `context_globs_for_agent("warp")` row in `backend/mod.rs` (`AGENTS.md`, `.agents/**`, `WARP.md`).
- An empty curated catalogue (`CliModelSource::Curated` returning no models) — the daemon accepts it.
- `worktree_removal_applies_to` in `tddy-session-activity/src/session_deletion.rs:44-49`.
- Web: `SessionType` union, `SESSION_TYPE_OPTIONS`, `createSessionRequest.ts` branch, **no model
  field** (`CreateSessionPane.tsx:369`), and **Create must not require a model** for warp-cli
  (`useCreateSessionSubmit.ts:60` — blocker B3).
- **Resume token (B4)** — no precedent: Cursor mints its chat id *before* spawn. Plumbing exists:
  `opencode_cli_spawn::launch()` returns the `PtyHandle` (`pty_done: watch::Receiver<bool>`,
  `capture: Arc<Mutex<TerminalCapture>>`, 64 KiB, readable after exit). A watcher after `launch()`
  at start and resume can parse the last `warp --resume <TOKEN>` and write it to `.session.yaml`.
  Generic resume passes no `conversation_id` today (`opencode_cli_spawn.rs:337-341`).
- **`SessionMetadata`** is `deny_unknown_fields` with no `Default`; a new field touches ~50 struct
  literals in 36 files (B5).
- Provider-route blockers B1 (`route_on_account` refuses an empty model) and B2 (connect always probes
  an HTTP endpoint) **vanish** with the provider preset dropped — `warp-cli` refuses every preset.

### What an agent terminal tab touches (Exploration 3)

Bash is the only kind, end to end: `StartTerminalSessionRequest` has no kind/command field; the RPC
layer resolves the login shell and calls `TerminalRoster::start(session_id, shell_path)`;
`CliSessionManager::start_terminal` hardcodes kind `"bash"`, `argv=[shell]`, no env, `os_user=None`.

Minimal seam: `string kind = 4;` on `StartTerminalSessionRequest` (empty = bash); widen
`TerminalRoster::start` to a launch spec; the daemon roster maps `kind` → `cli_harness(kind)` →
`(harness.argv)(&CliLaunch { binary, session_id: <fresh>, ..default })` with binary from
`DaemonConfig::cli_harness_binary`; a new `CliSessionManager` method passes `kind` to `spawn_tool`.
`ListTerminalSessions` then reports the kind for free (`resolve_pty_handle` reads `control.kind`).
Web: `useSessionTerminals` keeps `{id, kind}`, kind-aware labels (today `id.replace(/-/g," ")`, i.e. a
UUID for real daemon tabs), and the `+` becomes a menu.

Constraints found: sandboxed sessions cannot start any extra tab (`FAILED_PRECONDITION "session has no
running terminal"`); the coder (room-backed) roster serves bash only; tabs are in-memory (no
persistence, no resume); `DeleteSession` / `SignalSession` do not stop tabs; tabs run with the
daemon's identity and **no session env**.

### Deferred-work cross-check (Step 2b)

No `**Claimed by:**` record in any package this stack touches. Records in the path, none blocking:

| Record | Verdict |
|---|---|
| `packages/tddy-agent-backend/docs/code-issues/oversized-file-backend-mod.md` (906 prod lines) | ⚠ during — node 1 adds one `context_globs_for_agent` row; record the +1, split deferred (unclaimed, large) |
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-resume-session-resume-session-at-session-coordinate.md` | ⚠ during — node 1 must not add a branch; the registry lookup already covers warp-cli |
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md` | ⚠ during — same; no new branch |
| `packages/tddy-worktree-service/docs/code-issues/complexity-branch-intent-resolve-branch-workflow.md` | — unrelated (generic policy, untouched) |
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-sandboxed-cli-harness-session-start-sandboxed-cli-harness-session.md` | — unrelated (sandbox out of scope) |
| `packages/tddy-web/docs/code-issues/oversized-file-ghostty-terminal*.md` | — unrelated (node 2 touches the tab strip, not Ghostty) |
| [2026-10-22-muse-code-cli-facts-unconfirmed.md](../todo/2026-10-22-muse-code-cli-facts-unconfirmed.md) | ℹ precedent — Warp's flags are confirmed from the official reference, but no real `warp` binary is on this machine; same "pinned as written" risk |
| [2026-10-22-sandbox-app-cannot-start-opencode-or-muse.md](../todo/2026-10-22-sandbox-app-cannot-start-opencode-or-muse.md) | — unrelated (sandbox out of scope) |
| [2026-10-21-web-provider-preset-options-mirror-the-daemon-table.md](../todo/2026-10-21-web-provider-preset-options-mirror-the-daemon-table.md) | — unrelated now (no preset added) |
| [2026-08-13-session-deletion-leaks-the-worktree-for-every-session-type-except-clau.md](../todo/2026-08-13-session-deletion-leaks-the-worktree-for-every-session-type-except-clau.md) | ⚠ during — node 1 adds `warp-cli` to `worktree_removal_applies_to` so it does not join the leakers |

Packages with no `docs/code-issues/`: `tddy-daemon`, `tddy-accounts`, `tddy-credentials` (not
analyzed ≠ clean). `tddy-changeset`, `tddy-cli-sessions`, `tddy-terminal-rpc` (has one record,
unrelated) — see node docs.

---

## Exploration 1 — parent-driven reads (orientation)

**Sequence**

1. `cat .agents/skills/planning/references/planning-phase.md`, `.agents/skills/deferred-work/references/planning-cross-check.md` — process.
2. `ls packages`; `grep -rli warp` over packages/docs — **no hits**; `ls docs/dev/1-WIP docs/ft`.
3. `grep -rn "claude-cli|codex-acp|cursor-agent"` in `tddy-agent-backend/src`, `tddy-session-agents/src`, `tddy-agent-launch/src` — found `cli_harness.rs`, `provider_route.rs`, `model_catalog.rs`.
4. `cat packages/tddy-agent-backend/src/cli_harness.rs` — the registry (4 descriptors, `CliLaunch`, `CliModelSource`, argv fns).
5. `cat docs/ft/daemon/muse-cli-session.md` — the latest harness precedent's feature doc.
6. `ls docs/dev/changesets | grep muse|opencode|cursor-cli|harness|roster`; `git log --all | grep muse` — #667 (`f6b550f9e`), #670 (`1cbdff00c`), `3e95c8447`.
7. `git show --stat f6b550f9e`; `cat cli_harness_muse.rs` — descriptor template.
8. `sed -n 1,200p docs/ft/daemon/session-agent-roster.md` — roster agents are OpenAI-compatible model loops (`SpecializedAgentDef { model, base_url, api_key, … }`), not CLI harnesses.
9. `grep -rln "additional agent|add agent|…" docs/ft`; `sed -n 1,80p docs/ft/coder/spawn-conversation.md`; `docs/ft/web/changelog/2026-07-27-session-agents-spawn-peer-agent-sessions-on-the-same.md`; `sed -n 690,870p docs/ft/web/session-drawer.md` — "Add agent" today attaches a **roster agent** (conversation tab); the peer-spawn flow on the same worktree was removed 2026-08-29.
10. `sed -n 1,80p packages/tddy-session-agents/src/spawn_agent_def.rs`; `grep struct SpecializedAgentDef` in `tddy-discovery/src/agent_def.rs:107` — confirms roster agents need `base_url`.
11. Web search + `WebFetch https://docs.warp.dev/_llms-txt/warp-agent-cli.txt` (both halves) and `https://docs.warp.dev/agents/cli/reference/` — Warp facts above.
12. `which warp oz warp-cli`; `ls /Applications/Warp.app/Contents/{MacOS,Resources/bin}`; `oz --help`, `oz agent --help`, `oz agent run --help`, `oz --version`.
13. `sed -n 1,80p packages/tddy-agent-backend/src/claude_argv.rs` — `build_claude_base_argv` omits `--model` when empty and uses `--session-id <id>` on a fresh start (relevant to a claude-cli agent tab).
14. Step 2b: `ls packages/<pkg>/docs/code-issues/` for 11 packages; `grep -rl 'Claimed by:' packages/*/docs/code-issues/` (hits only in tddy-code-restructuring, tddy-zed-extensions, tddy-workflow-recipes — none in this path); `grep -rl -iE '(muse|opencode-cli|cli.harness|terminal tab|bash tab|provider.account|cursor-cli|warp)' docs/dev/todo/`; read heads of the records in the table above.

**Findings** — "additional agent" has three candidate meanings in this codebase (roster agent,
agent terminal tab, peer session on the same worktree); the developer chose the agent terminal tab.

---

## Exploration 2 — CLI harness wiring (Explore agent, very thorough)

### Sequence

1. `git show --stat f6b550f9e` / `3e95c8447` — precedent file list.
2. Grep `muse-cli|muse_cli|MuseCli|TDDY_MUSE` (repo minus docs/target/node_modules) with per-file counts — found sites beyond #667 added by #670 (sandbox-recipes, sandbox-runner, discovery, daemon-kernel config).
3. Read `tddy-agent-backend/src/cli_harness.rs`, `cli_harness_muse.rs`, `lib.rs` — registry, descriptor, argv.
4. Read `provider_route.rs` — Harness enum, route matrix, route bodies.
5. Grep `backend/mod.rs` for context globs / model catalogues; read 715-814.
6. Read `backend/model_catalog.rs` — ListAgentModels resolution for CLI harnesses.
7. Grep callers of `context_globs_for_agent`, `.context_agent`, `curated_models_for_agent`.
8. Grep `muse` in agent-launch, daemon, daemon-kernel, discovery, sandbox-recipes, sandbox-runner, session-activity, worktree-service.
9. Grep `start_session_core` dispatch; read `svc_start_session_core.rs` 130-219, 340-496.
10. Read `cli_branch_starts.rs` 1-60, 230-341 — `start_cli_harness`, `start_cli_harness_from_request`.
11. Read `svc_resume_session.rs` (whole) — resume dispatch.
12. Read `opencode_cli_spawn.rs` (whole) — generic harness start/resume, `ResumedSession`, `harness_binary`.
13. Grep `conversation_id|cursor_chat_id|chat_id` (non-test).
14. Read `resume.rs`, `chat.rs` — Cursor chat-id minting/persist.
15. Grep `cursor_chat_id` everywhere; locate `SessionMetadata`.
16. List tddy-cli-sessions files; grep PTY exit / broadcast handling.
17. Read `cli_session_manager/launch.rs` 1-120, `pty_handle.rs` 1-80.
18. Read `pty_spawn.rs` 20-189 — `spawn_tool`, `spawn_terminal_cleanup`.
19. Read `tddy-task/src/terminal_capture.rs` 1-80 + pub fns; `task.rs` 190-235.
20. Read `tddy-changeset/src/session_metadata.rs` 1-140.
21. List every file with a `cursor_chat_id:` literal (36 files, 50 sites); read `service_util.rs` 80-110.
22. Read `cursor_cli_resume_chat_acceptance.rs` header + test names.
23. Grep existing PTY-output scraping (`buffered_bytes`, `strip_ansi`, `stdout_tx.subscribe`) and `pty_done`/`MAIN_TERMINAL_ID` consumers.
24. Read `cli_route_launch.rs` (whole).
25. Read `session_provider_route.rs` 25-184 — `route_on_account` blank-model refusal.
26. Grep `"meta"`; locate preset table.
27. Read `tddy-accounts/src/provider_account.rs` 1-260, 300-460.
28. Read web `ConnectProviderAccountForm.tsx`, `ProviderAccountDefaultsControl.tsx`.
29. Read `tddy-accounts/src/service.rs` 160-340, 637-680.
30. Read `tddy-daemon/src/provider_model_probe.rs` 1-90.
31. Read `tddy-daemon/src/provider_route_support.rs` (whole).
32. Read `branch_intent.rs` 60-120, `session_deletion.rs` 25-60.
33. Read `daemon-kernel/src/config.rs` 145-175, `config/agent_cli.rs` 155-214.
34. Read `harness_profile_start.rs`, `cli_harness_jail.rs`.
35. Read sandbox-runner `harness_pty.rs` 1-80, `runner.rs` 280-300, 2410-2440.
36. Read `svc_start_sandboxed_cli_harness_session.rs` 40-120; grep `sandboxed_agent_kind(`.
37. Read sandbox-recipes `plan.rs`, `route_files.rs`, `muse_cli.rs`.
38. `git log` on jail files; `git show --stat 1cbdff00c` (#670).
39. Grep web `muse-cli|opencode-cli|SESSION_TYPE_OPTIONS`.
40. Read `CreateSessionTypeToggle.tsx`, `createSessionRequest.ts`, `CreateSessionPane.tsx` 150-389.
41. Read `useCreateSessionModel.ts`, `useCreateSessionSubmit.ts` 40-80, `CreateSessionModelField.tsx`.
42. Grep daemon `ListAgentModels` path; `agent_models.rs`; `useAgentModels.ts` empty handling.
43. List test names in muse/opencode/sandboxed test files; read bodies of registry/route tests.
44. Read `CreateSessionMuseAcceptance.cy.tsx`, `ProviderAccountDefaultsAcceptance.cy.tsx` (muse lines), `testIds.ts`.
45. Grep `.proto` for session-type strings.
46. Grep hard-coded `"opencode-cli"`/`"cursor-cli"` outside tests (Rust + web).
47. Read web `constants/claudeCliModels.ts`, `SessionsDrawerScreen.tsx` 195-225, `attachClaim.ts` 40-60, `session_coordinate_handlers.rs` 455-490, `catalog/ports.rs` 200-220.
48. Grep signals used by stop/delete (`svc_signal_delete_session.rs`, `session_deletion.rs`).
49. List docs: `docs/ft/daemon/*`, `docs/dev/todo/*muse*`, `docs/dev/1-WIP`.
50. Read `context_files.rs` 55-90, `tddy-tools/src/list_models.rs` 15-25, `tddy-discovery/src/harness_profile.rs` 1-20.
51. Read `context_globs_acceptance.rs` rstest case lists.

### Inspected files (excerpts)

**`packages/tddy-agent-backend/src/cli_harness.rs`**
- `CliLaunch` (16-35): `binary`, `model`, `session_id`, `conversation_id: Option<&str>` ("The harness's own conversation id to continue (Cursor's chat id)"), `initial_prompt`, `permission_mode`, `dangerously_skip_permissions`, `resume`.
- `CliModelSource` (39-44): `Curated(fn() -> BackendModels)` | `ListedByBinary(&[&str])` — no "no models" variant.
- `CliHarness` (48-67): `session_type, label, route_harness: Harness, context_agent, default_binary, binary_env, route_config_env: Option<&str>, argv: fn(&CliLaunch)->Vec<String>, models`.
- `static CLI_HARNESSES: [CliHarness; 4]` (70); entry 104 = `MUSE_CLI`.
- `cursor_cli_argv` (167-181): `--resume <chat>` only for a non-blank chat id — the precedent for `warp --resume <TOKEN>`.

**`packages/tddy-agent-backend/src/cli_harness_muse.rs`** (64 lines)
```rust
pub const MUSE_ROUTE_KEY_ENV: &str = "META_API_KEY";            // :15
pub const MUSE_CLI: CliHarness = CliHarness { session_type: "muse-cli", label: "Muse Code",
    route_harness: Harness::MuseCli, context_agent: "muse", default_binary: "muse",
    binary_env: "TDDY_MUSE_CLI", route_config_env: None, argv: muse_argv,
    models: CliModelSource::Curated(muse_cli_models) };          // :54-64
```
Registered in `lib.rs:11` (`pub mod cli_harness_muse;`).

**`packages/tddy-agent-backend/src/provider_route.rs`**
- `enum Harness` (22-43) includes `MuseCli`; `id()` (55-68); `const ALL: [Harness; 10]` (70-81); `product()` (84-92) — each needs a new arm.
- `route_supported` (209-233): `Harness::MuseCli if preset == META_PRESET => Ok(())`, `Harness::MuseCli => Err(RouteRefusal::PresetNotAccepted{…})`.
- `const META_PRESET: &str = "meta";` (238). `route_body` (269-279): `Harness::MuseCli => Ok(muse_route)`. `muse_route` (341-353): `secret_env: vec![(MUSE_ROUTE_KEY_ENV, api_key)]`.
- `PresetNotAccepted` Display (191-195) hard-codes "it runs on Meta only".

**`packages/tddy-agent-backend/src/backend/mod.rs`** — `SHARED_CONTEXT_GLOBS = ["AGENTS.md", ".agents/**"]` (725); `context_globs_for_agent` (755-780), muse row 777 `"muse" => &["AGENTS.md", ".agents/**", ".muse/**"]`. `claude_cli_models` (789), `cursor_cli_models` (800).

**`packages/tddy-agent-backend/src/backend/model_catalog.rs`** — `resolve_agent_models` (94-111): `if let Some(harness) = cli_harness(agent)` → `Curated(c) => Ok(c())` | `ListedByBinary` runs the binary. `BackendCliPaths::listing_binary_override` (42-47) matches only `opencode-cli`. An empty `Curated` catalogue is accepted (`tddy-daemon-rpc/src/catalog/agent_models.rs:55-56` only rejects malformed output).

**`packages/tddy-agent-launch/src/svc_start_session_core.rs`** — 141-150 model-required check only for `claude-cli`/`cursor-cli`; 347-384 claude-cli; 386-422 cursor-cli; 424-429:
```rust
// --- any other registered CLI harness (opencode-cli, muse-cli): a PTY, no LiveKit ---
if let Some(harness) = cli_harness(req.session_type.trim()) {
    return self.start_cli_harness(harness, &req, progress, os_user).await;
}
```

**`packages/tddy-agent-launch/src/cli_branch_starts.rs`** — `start_cli_harness` (252-278): refuses `managed_codebase`, `cli_start_prelude`, sandboxed vs PTY, `announce_worktree_ready`. `start_cli_harness_from_request` (281-340): seeded roster, `session_route_sources_with_default(req, harness.session_type, …)`, → `opencode_cli_spawn::start_cli_harness_session(CliHarnessStart{…})`.

**`packages/tddy-agent-launch/src/opencode_cli_spawn.rs`**
- `harness_binary` (76-82) → `config.cli_harness_binary(session_type, default_binary, binary_env)`.
- `start_cli_harness_session` (86-257): 126 `start_cli_route`; 136 `BranchIntentPolicy::cli_harness`; 146 `provision_cli_worktree`; 206 `launch_model_for`; 207-221 `launch(... CliLaunch { binary, model, initial_prompt, ..default })`; 225-238 `SessionMetadata { model: Some(model).filter(non-empty), .. starting_session_metadata(...) }` → `write_session_metadata`.
- `resume_cli_harness` (262-289); `struct ResumedSession` (293-300); `resume_cli_harness_session` (305-360) — **no `conversation_id`**:
  ```rust
  &CliLaunch { binary: &binary, resume: true, ..CliLaunch::default() },   // :337-341
  ```
  then `updated.pid`, `status = "active"`, `updated_at` (347-352).
- `launch()` (363-384) → `cli_manager.launch_cli_harness(harness, session_id, worktree, launch, route.args, env)` → `Arc<PtyHandle>`. Natural exit-watcher attach point (`session_dir` in scope at both callers).

**`packages/tddy-agent-launch/src/svc_resume_session.rs`** — 55-68 claude-cli; 70-89 cursor-cli; 91-110 generic harness: `metadata.sandbox == Some(true)` → `resume_sandboxed_cli_harness`, else `resume_cli_harness(harness, …)`.

**`packages/tddy-agent-launch/src/cli_route_launch.rs`** — `launch_model_for` (37-43); `start_cli_route` (83-104) → `route_on_account(..., model)`; `resume_cli_route` (109-122).

**`packages/tddy-agent-launch/src/session_provider_route.rs:95-99`** — blank model refused for a routed session (B1; moot once warp-cli refuses every preset).

**`packages/tddy-agent-launch/src/harness_profile_start.rs:28`** — exhaustive `Harness::ClaudeCli | OpenCodeCli | MuseCli | CursorCli => session type` (compile-forced arm).

**`packages/tddy-agent-launch/src/cli_harness_jail.rs`** — `sandboxed_agent_kind` (17-23) `opencode-cli → opencode`, `muse-cli → muse`, `_ => None`; `resolve_cli_jail_binaries` (126-131) refuses `None` ("the sandbox runner does not host {session_type}"). `svc_start_sandboxed_cli_harness_session.rs:55-90` resolves the route (needs model + account) *before* that refusal — a sandboxed warp-cli with no account fails with "needs a provider account".

**Cursor chat-id precedent** — `resume.rs:24-101` (`meta.cursor_chat_id` or `chat::mint_cursor_chat_id` "adopted", → `resume_cursor(..., Some(&chat_id))`, persisted); `chat.rs:59-101` (`parse_created_chat_id`, `mint_cursor_chat_id` runs `<binary> create-chat` before spawn); `cursor_cli_spawn.rs` 260-264, 350, 383-405.

**PTY layer** — `cli_session_manager/launch.rs:21-43` `launch_cli_harness` → `spawn_tool(session_id, MAIN_TERMINAL_ID, harness.session_type, …)`; `pty_handle.rs:24-52` `PtyHandle { stdout_tx, capture: Arc<Mutex<TerminalCapture>>, pid, pty_done: watch::Receiver<bool>, … }`; `pty_spawn.rs` 109-121 (`pty_done` set on terminal status), 140-185 `spawn_terminal_cleanup` (no per-harness exit hook); `tddy-task/src/terminal_capture.rs` `CAPTURE_LIMIT_BYTES = 64 * 1024` (43), `buffered_bytes()` (102), `replay_last(max)` (150). No PTY-output scraping exists; only ANSI stripper `tddy-vm/src/serial_shell.rs:353 strip_ansi_codes` and dev-dep `strip-ansi-escapes = "0.2"` in `tddy-e2e/Cargo.toml:35`.

**`packages/tddy-changeset/src/session_metadata.rs:10-122`** — `#[serde(deny_unknown_fields)] pub struct SessionMetadata`, **no `Default`**. 37-42:
```rust
/// Cursor chat id (`cursor-agent create-chat`) ...
#[serde(default, skip_serializing_if = "Option::is_none")]
pub cursor_chat_id: Option<String>,
```
50 `cursor_chat_id:` literal sites / 36 files. Production: `tddy-session-split/src/service_util.rs:99` (`starting_session_metadata`), `tddy-agent-launch/src/cursor_cli_spawn.rs`, `tddy-session-activity/src/{remote_git_pack_execution,session_deletion,session_list_enrichment,session_reader}.rs`, `tddy-session-files/src/host_documents.rs`, `tddy-telegram-control/src/telegram_session_control/cli_spawns.rs`, `tddy-testing-commons/src/builders.rs:254`; rest tests.

**Provider accounts** (moot for this stack) — `tddy-accounts/src/provider_account.rs` `presets()` (113-160, `[ProviderPreset; 6]`); `service.rs` `connect_provider_account` (192-221) always probes; `tddy-daemon/src/provider_model_probe.rs:54` `(None, None) => Err("the account serves no endpoint to list models from")`; `tddy-daemon/src/provider_route_support.rs` test `muse_code_runs_only_on_a_meta_account` (54-68).

**Worktree, deletion, config** — `branch_intent.rs:80-88` `BranchIntentPolicy::cli_harness(session_type)` generic. `session_deletion.rs:44-49`:
```rust
pub fn worktree_removal_applies_to(session_type: &str) -> bool {
    matches!(session_type, "claude-cli" | "opencode-cli" | "muse-cli" | "workspace") }
```
Delete: SIGTERM, wait, SIGKILL (182-194). `config.rs:159-162` `cli_harnesses: BTreeMap<String, CliHarnessConfig>`; `config/agent_cli.rs` 163-168 `CliHarnessConfig { binary_path }`, 191-213 `cli_harness_binary` (env → yaml → default) — generic.

**Web create form** — `createSessionRequest.ts:7` `SessionType = "tool" | "claude-cli" | "cursor-cli" | "opencode-cli" | "muse-cli"`; 171-197 opencode/muse branch (`initialPrompt: ""`, `sandbox: false`, `managedCodebase: false`, …). `CreateSessionTypeToggle.tsx:9-15` `SESSION_TYPE_OPTIONS` (testid `create-session-type-${id}`). `CreateSessionPane.tsx` 287-289 `modelField`, 320-328 provider field when `accountsClient && providerHarness !== null`, 369 `{(sessionType === "opencode-cli" || sessionType === "muse-cli") && modelField}`. `useCreateSessionModel.ts` 47-48 `modelAgentKey = sessionType`, 56-57 `providerHarness = sessionType`, 76-77, 81-83. `useCreateSessionSubmit.ts:50-61`:
```ts
// A model is always required ...
return Boolean(projectId && model);
```
(B3). `ProviderAccountDefaultsControl.tsx:11-15` `HARNESS_OPTIONS` (claude-cli, opencode-cli, muse-cli). `constants/claudeCliModels.ts:25-27` `isCliTerminalSession` (claude+cursor only, mobile shortcuts).

**Other session-type sites** — `tddy-session-files/src/context_files.rs:76-83` (automatic via registry); `session_coordinate_handlers.rs:471` activity tailing claude/cursor only; `hook_reports.rs:42` claude/cursor only; doc-comment lists in `tddy-tools/src/list_models.rs:19`, `tddy-discovery/src/harness_profile.rs:9`, `harness_profile_start.rs:10`, `daemon-kernel/src/config.rs:159`, `sandbox-runner/src/runner.rs:288-292`. Proto: `session_type` is a free `string` (`tddy-service/proto/session.proto:290, 461-462`); no enum.

**Tests from #667 / #670**
- `tddy-agent-backend/tests/muse_cli_acceptance.rs`: `muse_code_is_registered_as_a_cli_harness`, `a_muse_session_starts_with_its_model_and_first_prompt`, `a_resumed_muse_session_continues_its_session`, `muse_on_meta_is_handed_the_key_and_nothing_else`, `muse_accepts_only_meta_accounts`, `the_curated_muse_models_default_to_spark`.
- `cli_harness_acceptance.rs`: `the_registry_offers_claude_cursor_opencode_and_muse_in_order` (exact vec), `a_tool_or_workspace_session_is_no_cli_harness`, opencode tests.
- `provider_route_acceptance.rs`: `every_harness_id_names_its_harness_and_back` (143-165), `every_harness_but_cursor_answers_routable` (230-250).
- `context_globs_acceptance.rs` rstest cases 137-142, 164-170.
- `tddy-agent-launch/src/cli_route_launch_tests.rs`: `a_muse_launch_on_a_meta_route_keeps_the_sessions_model`, `an_unrouted_muse_launch_keeps_the_sessions_model`.
- `tddy-session-activity/tests/worktree_removal_eligibility.rs`: `a_muse_cli_session_is_eligible_for_worktree_removal` (60).
- `tddy-session-lifecycle/tests/opencode_cli_session.rs` (`CliHarnessDaemon`, `daemon_config_with_cli_harness_binary(session_type, binary)` at 671): `starting_a_muse_session_launches_muse_with_the_chosen_model`, `starting_a_muse_session_opens_with_its_first_prompt`, `a_muse_session_started_without_a_model_leaves_the_choice_to_muse`, `a_muse_session_records_its_type_model_and_worktree`, `a_sandboxed_muse_session_without_a_provider_account_is_refused`, `a_muse_session_over_a_managed_codebase_is_refused`, `resuming_a_muse_session_continues_its_session`, `resuming_a_muse_session_marks_it_active_again`, `resuming_a_muse_session_whose_worktree_is_gone_is_refused`, `a_muse_session_is_listed_with_its_agent_and_model`.
- `tddy-session-lifecycle/tests/cursor_cli_resume_chat_acceptance.rs` (stub agent from `tddy_testing_commons::stub_scripts` — `a_stub_agent_script`, `read_last_appended_argv`): `starting_a_cursor_cli_session_records_the_chat_id_it_will_resume_into`, `resuming_a_cursor_cli_session_reattaches_to_its_original_chat`, `resuming_a_cursor_cli_session_does_not_replay_the_initial_prompt`, `resuming_a_session_started_before_chat_ids_were_recorded_adopts_a_chat_for_it`.
- Cypress `CreateSessionMuseAcceptance.cy.tsx` ("starts a muse-cli session with a Muse model"; stubs `listAgentModels` by `req.agent`); `ProviderAccountDefaultsAcceptance.cy.tsx` 101-119, 153-156; `testIds.ts:336` `createSessionTypeMuseCliBtn`.

### Grep / glob

| Pattern | Scope | Notable hits |
|---|---|---|
| `muse-cli\|muse_cli\|MuseCli\|TDDY_MUSE` | repo − docs/target/node_modules | 37 files; beyond #667: `sandbox-recipes/{lib,plan,route_files,muse_cli}.rs`, `sandbox-runner/src/harness_pty.rs`, `discovery/src/harness_profile.rs:9`, `daemon-kernel/src/config.rs:159`, `agent-launch/src/{cli_harness_jail,harness_profile_start,svc_*sandboxed_cli_harness*}.rs` |
| `context_globs_for_agent\|\.context_agent\|curated_models_for_agent` | packages *.rs | session-split (3), `sandbox-app/src/spawn.rs:570`, `svc_resolve_os_user.rs:82`, `context_files.rs:66,82` |
| `conversation_id\|cursor_chat_id\|chat_id` | non-test *.rs | `resume.rs:51-88`, `cursor_cli_spawn.rs:264-405`, `service_util.rs:99` |
| `cursor_chat_id:` | packages | 50 hits / 36 files |
| `buffered_bytes()\|\.replay()\|strip_ansi\|stdout_tx.subscribe` | non-test | `task.rs:205`, `pty_terminal_service.rs:58,69`, `terminal_session_adapter.rs:46,107,119`, `tddy-vm/serial_shell.rs:353` — no scraping |
| `fn cli_harness_binary` | daemon-kernel | `config/agent_cli.rs:191` |
| `muse-cli\|opencode-cli\|SESSION_TYPE_OPTIONS` | tddy-web/src | `CreateSessionTypeToggle.tsx:9-14`, `CreateSessionPane.tsx:369`, `createSessionRequest.ts:7,171`, `ProviderAccountDefaultsControl.tsx:13-14` |

### Findings

Registry-driven: one descriptor gives most of a harness. Gaps for Warp: B3 (web requires a model),
B4 (resume token printed on exit — no precedent; watcher on `pty_done` + `capture` after `launch()`
at start and resume; SIGKILL / daemon restart means no token; re-capture on every exit; no token →
fresh start, never a bare `--resume`), B5 (`SessionMetadata` field ≈ 50 literals). B1/B2 provider
blockers moot without a preset. Sandbox: leave warp-cli out of `sandboxed_agent_kind`.
`--auto-approve`: no per-harness flag channel except Claude's `dangerously_skip_permissions`.

---

## Exploration 3 — terminal tabs mechanism (Explore agent, very thorough)

### Sequence

1. `ls docs/ft/web docs/ft/daemon packages`, `find -iname "*terminal*"`, `find -name "*.proto"`.
2. `cat docs/ft/web/session-terminal-tabs.md`.
3. `cat -n packages/tddy-terminal-rpc/proto/terminal_session.proto`, `packages/tddy-service/proto/terminal.proto`.
4. `cat docs/ft/daemon/terminal-sessions.md`; `ls` the `tddy-terminal-rpc` crate.
5. `cat packages/tddy-terminal-rpc/docs/terminal-session-service.md`, `src/session.rs`, `src/login_shell.rs`.
6. Grep `TerminalRoster|start_terminal|login_shell|fn` in `service.rs`; read 1-440.
7. Grep `"TerminalRoster for"`; grep `'"bash"'` across packages.
8. `cat tddy-session-lifecycle/src/connection_service/svc_terminal_ports.rs`, `tddy-cli-sessions/src/cli_session_manager/terminals.rs`.
9. `ls cli_session_manager/`; grep `fn spawn_tool`, `stop_session(`.
10. `cat pty_spawn.rs`, `pty_handle.rs`.
11. `wc -l` cli-sessions sources; grep `spawn_tool|os_user|env` in `launch.rs`/`relaunch.rs`.
12. Read `launch.rs` 1-110; grep `launch_cli_harness`.
13. `find cli_harness*.rs`.
14. `cat tddy-agent-backend/src/cli_harness.rs`.
15. Grep `binary_env|cli_harnesses|binary_path`.
16. Read `opencode_cli_spawn.rs` 60-130, 330-420; grep `cli_harnesses` in daemon-kernel.
17. Read `config/agent_cli.rs` 100-240, `config.rs` 155-170.
18. `cat cli_harness_muse.rs`; grep `git_environment`.
19. Grep `agent_backend` in `tddy-core/src/lib.rs`; read `opencode_cli_spawn.rs` 180-300.
20. Read `tddy-coder/src/session_participant/terminal_manager.rs` 1-200, `terminal_session_service.rs` 200-300.
21. Grep `impl|fn|sandbox` in `tddy-session-lifecycle/src/terminal_session_adapter.rs`; read 186-230.
22. Read header + test names of `tddy-session-lifecycle/tests/terminal_session_acceptance.rs`.
23. Grep test names across terminal + harness suites.
24. Coder `tests.rs` names; `tddy-terminal-rpc/tests/support/mod.rs` 340-400 (`InMemoryRoster`).
25. Web: grep `sessions-terminal-tab`, `startTerminalSession`; `ls docs | grep term`.
26. `cat useSessionTerminals.ts`; grep `src/rpc/connections/terminal.ts`.
27. `cat SessionTerminalTabs.tsx`.
28. Grep users of `SessionTerminalTabs`/`useSessionTerminals`; test ids in `testIds.ts`.
29. Read `SessionRuntime.tsx` 1-340.
30. Grep cypress specs; list `SessionTerminalTabsAcceptance.cy.tsx` test names.
31. Read that spec (1-125, 123-200), `terminalSessionServiceBackend.ts`, `sessionTerminalTabsPage.ts`, `mountWithRpc`.
32. Read `daemonSessionHostBackend.ts` 240-300; visible-errors / fullscreen / agent-tabs spec names; `tddy-connectrpc-testkit` API.
33. Grep teardown callers (`stop_session`, `kill_all`) and `SessionMetadata`.
34. Read `tddy-agent-launch/src/svc_signal_delete_session.rs` 20-200.
35. Read `session_deletion.rs` `delete_session_directory`; `project_home_idle.rs` 125-150.
36. Read `tddy-terminal-rpc/src/pty_runtime.rs`, `tddy-pty/src/runtime.rs`.
37. Grep repo for `warp` — none; `git log -5`.
38. Read `tddy-terminal-rpc/build.rs`, `buf.gen.yaml`, `GrpcSessionTerminal` terminalId, `docs/terminal-session.md`.
39. Read `exposed_terminals.rs`; grep `terminal_session_entry` mount sites.
40. Grep remaining Rust tests touching terminal start; `task_service_acceptance` names; `serial_test`.

### Inspected files (excerpts)

**Docs** — `docs/ft/web/session-terminal-tabs.md`: Agent tab `sessions-terminal-tab-agent` (reserved id `"main"`, no close); bash tabs `sessions-terminal-tab-<terminalId>` + `sessions-terminal-tab-close-<terminalId>`; "+" `sessions-terminal-tab-new` → `StartTerminalSession`; panes kept mounted (`display:none`); per-session control mutex; non-goals: renaming tabs, persisting tabs across restart. `docs/ft/daemon/terminal-sessions.md`: AC3 "`StartTerminalSession(session_id)` starts a **Bash tool** … login shell in the session's worktree, with **no inputs**" (`pw_shell` → `$SHELL` → `/bin/bash`); AC6 stopping `"main"` → `INVALID_ARGUMENT`; AC2 `kind` `"claude-cli"` for main, `"bash"` for started; non-goals "Tools that take inputs. The API leaves room for them…". `packages/tddy-terminal-rpc/docs/terminal-session-service.md`: four ports (`TerminalSessionStore`, `TerminalSession`, `TerminalRoster`, `TerminalControl`); two servers share `build_terminal_session_entry`.

**Proto** — `packages/tddy-terminal-rpc/proto/terminal_session.proto` 24-28 RPCs; 165-170:
```proto
message StartTerminalSessionRequest {
  string session_token = 1;
  string session_id    = 2;
  string control_token = 3;
}
message StartTerminalSessionResponse { string terminal_id = 1; } // "Fresh identifier for the started login-shell terminal (never "main")"
```
195-200 `TerminalSessionInfo { terminal_id=1; kind=2 /* "claude-cli" | "bash" */; pid=3; }`. Codegen: Rust `build.rs` (prost + `TddyServiceGenerator` + tonic); web `packages/tddy-web/src/gen/terminal_session_pb.ts` via `buf.gen.yaml`; copy at `packages/tddy-rust-typescript-tests/gen/terminal_pb.ts`.

**`packages/tddy-terminal-rpc/src/service.rs`** — 97-104 `TerminalDescriptor { terminal_id, kind, pid }`; 113-124:
```rust
pub trait TerminalRoster: Send + Sync {
    async fn start(&self, session_id: &str, shell_path: &str) -> Result<String, Status>;
    async fn stop(&self, session_id: &str, terminal_id: &str) -> bool;
    async fn list(&self, session_id: &str) -> Vec<TerminalDescriptor>;
}
```
369-384:
```rust
// TODO(#unbundle-node-6): StartTerminalSession / StopTerminalSession document a control_token … but neither checks it
async fn start_terminal_session(...) {
    let os_user = self.os_user(&req.session_token)?;
    let session_id = keyed_session_id(&req.session_id);
    let shell_path = crate::login_shell::login_shell_for(&os_user);
    let terminal_id = self.ports.roster.start(&session_id, &shell_path).await?;
```
397-402 rejects `MAIN_TERMINAL_ID`; 414-435 list maps descriptor → info; 569 `build_terminal_session_entry(ports)`. `login_shell.rs:23-30` `DEFAULT_LOGIN_SHELL = "/bin/bash"`, `login_shell_for`. `pty_runtime.rs:42-53` `PtySpawnSpec { argv, worktree_path, session_id, terminal_id, kind, env, os_user }` — "Callers with no user context (e.g. Bash terminals) pass `None`". `tddy-pty/src/runtime.rs` 269-284 `cwd(worktree)`, `TERM=xterm-256color`, `COLORTERM=truecolor`, strips `GITHUB_TOKEN_VARIABLES`, applies `spec.env`; default 24×220.

**Daemon** — `svc_terminal_ports.rs` 33-38 `DaemonTerminalSessionStore::new(claude_cli_manager, sandbox_manager)`; 59-89 `terminal_session_entry`/`terminal_session_ports` wire `CliManagerTerminalRoster`; 142-158:
```rust
async fn start(&self, session_id: &str, shell_path: &str) -> Result<String, Status> {
    let main = self.manager.get(session_id).await
        .ok_or_else(|| Status::failed_precondition("session has no running terminal"))?;
    let worktree = main.worktree_path.clone();
    let handle = self.manager.start_terminal(session_id, worktree, shell_path).await …;
    Ok(handle.terminal_id.clone())
}
```
`tddy-cli-sessions/src/cli_session_manager/terminals.rs` 24-43:
```rust
pub async fn start_terminal(&self, session_id, worktree_path, shell_path) -> … {
    let terminal_id = uuid::Uuid::now_v7().to_string();
    let argv = vec![shell_path.to_string()];
    self.spawn_tool(session_id, &terminal_id, "bash", worktree_path, "", argv, Vec::new(), None).await
}
```
76-95 `stop_terminal`; 101-118 `stop_session`; 125-208 `kill_all`. `pty_spawn.rs` 29-91 `spawn_tool(session_id, terminal_id, kind, worktree_path, model, argv, env, os_user)` (`pub(crate)`); 140-185 cleanup (only `MAIN_TERMINAL_ID` exit drops managed workflows; bash tabs survive main's exit); 187-216 `resolve_pty_handle` (kind from PTY registry). `pty_handle.rs:24-52` `kind /* "claude-cli" | "bash" */`. `launch.rs` 21-43 `launch_cli_harness` → always `MAIN_TERMINAL_ID`. `terminal_session_adapter.rs` 186-212 composite store: sandbox first, `terminal_id != MAIN_TERMINAL_ID → None`. `svc_signal_delete_session.rs` 26-113 SignalSession signals `metadata.pid` only; 116-200 DeleteSession does not call `stop_session`. `project_home_idle.rs:141` sole non-test `stop_session` caller. `SessionMetadata` has no terminal fields.

**Coder** — `terminal_manager.rs` 105-180 `start_terminal` (`argv: vec![shell_path]`, `kind: "bash"`); `terminal_session_service.rs` 236-288 `CoderTerminalRoster::start` (refuses mismatched `session_id`), list returns only started shells.

**Harness registry / binary** — `CliLaunch` derives `Default`; `(harness.argv)(&CliLaunch { binary: &binary, ..CliLaunch::default() })` is a fresh interactive start. `DaemonConfig::cli_harness_binary(session_type, default_binary, binary_env)` (`agent_cli.rs:189-214`); `harness_binary()` in `opencode_cli_spawn.rs:76` is `pub(crate)` in `tddy-agent-launch`, while the daemon roster lives in `tddy-session-lifecycle` (which has `DaemonSessionHost.config`).

**Web** — `useSessionTerminals.ts` 12 `AGENT_TERMINAL_ID = "main"`; 16 `terminals: string[]`; 65-94 list drops `kind`; 100-122 `open()` → `startTerminalSession({ sessionToken, sessionId, controlToken })`; 129-152 `close()`; 124-127 `dropEnded`. `SessionTerminalTabs.tsx` 49-52:
```ts
function terminalLabel(terminalId: string): string { return terminalId.replace(/-/g, " "); }
```
99-107 Agent tab; 109-133 bash tabs; 186-194 `sessions-terminal-tab-new` (`aria-label="Open a new terminal"`); 198-214 fullscreen. `SessionRuntime.tsx` 77-80/104-107 `connection?.clientFor(TerminalSessionService)`; 109-121 `useSessionTerminals`; 225-240 tabs + `sessions-terminal-error`; 263-312 Agent pane; 317-333 one `GrpcSessionTerminal terminalId={id}` per tab. `testIds.ts:668-676,752,1170-1187`. `pages/sessionTerminalTabsPage.ts` (`tabs`, `agentTab`, `newTab`, `tab(id)`, `tabClose(id)`, `pane(id)`, `paneTerminal(id)`, `fullscreenToggle`). `rpc/terminalSessionServiceBackend.ts` 28-48 scenario (`terminals`, `newTerminalId(index)` default `bash-<n+1>`), 50-71 recorders (`startTerminalSessionIds`, `startedTerminalIds`, `stoppedTerminals`, …), 120-126 `startTerminalSession` always `kind: "bash"`. `daemonSessionHostBackend.ts:254-263` `aSessionServiceBackend` composes it. `inMemory.tsx:39-52` `mountWithRpc`. `tddy-connectrpc-testkit/src/backend.ts` `anInMemoryRpcBackend()` (276), `.failWith` (150).

### Grep / glob

| Pattern | Scope | Notable hits |
|---|---|---|
| `TerminalRoster for` | *.rs | `svc_terminal_ports.rs:142`, `tddy-coder/.../terminal_session_service.rs:236`, `tddy-terminal-rpc/tests/support/mod.rs:357` |
| `"bash"` | non-test *.rs | `cli_session_manager/terminals.rs:35`, `tddy-coder/.../terminal_manager.rs:118,153`, `tddy-pty/src/runtime.rs:381` |
| `fn spawn_tool` | cli-sessions | `pty_spawn.rs:29` |
| `launch_cli_harness` | *.rs | only caller `opencode_cli_spawn.rs:374` |
| `.stop_session(\|.kill_all()` | non-test | `project_home_idle.rs:141`, `tddy-daemon/src/runtime/tasks.rs:34`, `svc_shut_down_children.rs:23`, `tddy-desktop/src-tauri/src/lib.rs:286` |
| `sessions-terminal-tab` | web | `SessionTerminalTabs.tsx`, `testIds.ts` |
| `startTerminalSession` | web | `useSessionTerminals.ts`, gen, `terminalSessionServiceBackend.ts`, `SessionTerminalTabsAcceptance.cy.tsx`, `SessionTabsVisibleErrorsAcceptance.cy.tsx`, `commands.ts`, `cypress/e2e/ghostty-*.cy.ts` |
| `terminal_session_entry` | *.rs | `tddy-daemon/src/runtime/mount_entries.rs:17`, `mount_socket.rs:25`, `svc_session_files_ports.rs:84` |

### Tests

Rust — `tddy-session-lifecycle/tests/terminal_session_acceptance.rs` (real PTYs, `#[serial_test::serial]`, `DaemonSessionHost::new(...).terminal_session_service()`, stub main `exec cat`, `SHELL="/bin/sh"`, `a_capture_showing`): `start_terminal_registers_under_fresh_id_distinct_from_main` (139), `list_terminals_includes_main_reserved_id_and_started_shell` (166), `started_terminal_runs_login_shell_in_worktree` (203), `get_terminal_resolves_started_terminal_by_id` (231), `stop_terminal_kills_process_and_deregisters` (267), `start_terminal_session_returns_fresh_terminal_id` (318), `list_terminal_sessions_returns_main_and_started` (347), `stop_terminal_session_removes_from_list` (391), `stop_terminal_session_rejecting_main_returns_invalid_argument` (440), `terminal_session_rpcs_require_valid_token` (468), `stream_terminal_output_routes_by_terminal_id` (511), `send_terminal_input_targets_identified_terminal` (571). `tddy-terminal-rpc/tests/terminal_session_service_acceptance.rs` (`TerminalServiceHost`, `InMemoryRoster` records `started_shells()`): `start_terminal_session_answers_at_the_registered_service` (188), `stop_terminal_session_answers_at_the_registered_service` (206), `list_terminal_sessions_answers_at_the_registered_service` (236), `refuses_to_stop_the_main_terminal_through_the_terminal_lifecycle` (450). Coder `connection_service_participant/tests.rs`: `start_terminal_session_returns_a_fresh_non_main_terminal_id` (209), `start_terminal_session_refuses_a_request_naming_another_session` (242), … `tddy-coder/tests/two_server_parity_acceptance.rs` (747, 777). `sandbox_terminal_parity_acceptance.rs` `refuses_a_terminal_of_a_jail_that_only_has_the_main_one` (556).

Cypress — `SessionTerminalTabsAcceptance.cy.tsx`: "shows a fixed Agent tab that is selected by default and has no close control", "opens a new bash terminal via '+', focuses its tab, and streams its terminal_id", "supports multiple terminals — a second '+' yields two bash tabs, both kept mounted", "closing a bash tab stops that terminal and returns focus to the Agent tab", "routes keyboard input to the active terminal's terminal_id". `SessionTabsVisibleErrorsAcceptance.cy.tsx` "Terminal tab strip" (4 tests, `backend.failWith(TerminalSessionService.method.startTerminalSession, …)`). `SessionTerminalFullscreenAcceptance.cy.tsx`. `SessionTerminalTabsAgentTabs.cy.tsx` (conversation tabs, separate family).

### Findings — the minimal seam

1. Proto `string kind = 4;` on `StartTerminalSessionRequest` (empty = bash); optionally echo `kind` in the response. Regenerate web gen + Rust via `build.rs`.
2. `tddy-terminal-rpc/src/service.rs`: widen `TerminalRoster::start` to a request/spec (kind + resolved login shell); three impls (daemon, coder — refuse non-bash, test `InMemoryRoster`).
3. `svc_terminal_ports.rs` `CliManagerTerminalRoster`: take `DaemonConfig`, map kind → descriptor, argv via `CliLaunch` + `cli_harness_binary`; new `CliSessionManager` method passing `kind` to `spawn_tool`. `ListTerminalSessions` reports kind for free.
4. Web: `open(kind?)`, `{id, kind}` state, kind-aware labels, menu beside/replacing `sessions-terminal-tab-new`; fake records `req.kind`.
5. Gaps to decide: sandboxed sessions can't start any extra tab; tabs run without `os_user`/session env; not stopped on DeleteSession or main exit; no persistence.
