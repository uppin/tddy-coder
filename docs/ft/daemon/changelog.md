# Daemon product area changelog

## 2026-04-06 — Telegram user ↔ GitHub identity (library)

- **`tddy-daemon`**: Module **`telegram_github_link`** — **`TelegramOAuthStateSigner`** (HMAC-SHA256 OAuth **`state`** bound to **`telegram_user_id`**), **`TelegramGithubMappingStore`** (JSON on disk, atomic replace), **`resolved_os_user_for_telegram_workflow`**, **`complete_telegram_link_via_stub_exchange`** (**`StubGitHubProvider`**). **`TelegramSessionControlHarness::with_telegram_github_link`** optional mapping path; **`handle_start_workflow`** rejects unlinked Telegram users when that path is set (error text references **`/link-github`** / web OAuth). Dependencies: **`base64`**, **`hmac`**, **`sha2`**, **`subtle`**.
- **Feature doc**: [telegram-session-control.md](telegram-session-control.md). Package: [telegram-github-link.md](../../packages/tddy-daemon/docs/telegram-github-link.md), [changesets.md](../../packages/tddy-daemon/docs/changesets.md).

## 2026-04-06 — Telegram: concurrent elicitation (one chat, active token)

- **Coordinator:** **`ActiveElicitationCoordinator`** maintains a per-chat FIFO queue of workflow sessions; the head session owns the **active elicitation token** for Telegram interactive surfaces.
- **Outbound:** **`TelegramSessionWatcher`** registers elicitation requests on **`ModeChanged`**; sessions that are not primary for a chat receive a **deferred** text notice without a competing full **`eli:s:`** inline keyboard.
- **Inbound:** **`telegram_bot`** applies the same **active-token** policy to **`eli:s:`**, **`eli:o:`**, and **`doc:`** callbacks; **`/answer-text`** and **`/answer-multi`** check the active session before **`PresenterIntent`** calls. **`telegram_session_control`** advances the queue after completion on select, Other follow-up, applicable document-review actions, and successful text/multi answers.
- **Observability:** Deep per-chat queues trigger a **warning** log at a fixed depth threshold.
- **Feature docs:** [telegram-session-control.md](telegram-session-control.md), [telegram-notifications.md](telegram-notifications.md). Package: [telegram-notifier.md](../../packages/tddy-daemon/docs/telegram-notifier.md), [changesets.md](../../packages/tddy-daemon/docs/changesets.md).

## 2026-04-06 — Telegram `/start-workflow`: branch/worktree intent step

- **`tddy-daemon`**: After a recipe is saved (excluding **More recipes** follow-up), the bot prompts for **branch/worktree intent** (**New branch + worktree** vs **Work on existing branch**). The choice is written to **`changeset.yaml`** under **`workflow.branch_worktree_intent`** (`new_branch_from_base` / `work_on_selected_branch`) before project selection. Inline **`callback_data`** uses compact **`intent:nb|s:<session_id>`** and **`intent:ws|s:<session_id>`** so payloads stay within Telegram’s 64-byte limit with a UUID session id.
- **Feature doc**: [telegram-session-control.md](telegram-session-control.md). Package history: [changesets.md](../../packages/tddy-daemon/docs/changesets.md).

## 2026-04-05 — Telegram: inbound session control, PresenterIntent, elicitation UX

- **Inbound control**: Daemon runs **`telegram_bot`** (teloxide long-polling) when Telegram is configured and **`sessions_base`** resolves. Commands include **`/start-workflow`**, **`/sessions`**, **`/delete`**, **`/submit-feature`**, **`/answer-text`**, **`/answer-multi`**; callbacks cover session list, recipe/project/agent picks, document review (**`doc:`**), and elicitation select (**`eli:s:`**). **`TelegramSessionControlHarness`** and integration tests exercise the library; production uses **`TeloxideSender`** with the same bot as outbound notifications.
- **PresenterIntent**: **`presenter_intent.proto`** and **`tddy-daemon::presenter_intent_client`** forward answers and document actions to the child **`tddy-coder`** on localhost gRPC.
- **Outbound notifications**: **`ModeChanged`** for document review / markdown viewer sends **full document body** (chunked), then **Approve** / **Reject** / **Refine** (and related) inline actions. **`Select`** clarification sends a **numbered option list** in the message body, **numeric** inline buttons, and a **post-tap confirmation** with the full chosen option text. Dedupe for identical **`ModeChanged`** payloads per session is unchanged.
- **Formatting**: Styled text must follow Telegram **[message entities](https://core.telegram.org/api/entities)** rules (UTF-16 code units for offsets and lengths where applicable).
- **Feature docs**: [telegram-session-control.md](telegram-session-control.md), [telegram-notifications.md](telegram-notifications.md).

## 2026-04-05 — Telegram extended recipe keyboard: `review`

- **`tddy-daemon`**: **`RECIPE_MORE_PAGE`** includes the **`review`** workflow recipe name (same normalization rules as other CLI recipe strings).
- **Cross-reference**: [workflow-recipes.md](../coder/workflow-recipes.md) (**Selecting a recipe**); package [changesets.md](../../packages/tddy-daemon/docs/changesets.md).

## 2026-04-04 — Session elicitation: Telegram `ModeChanged` + `ListSessions` flag

- **`connection.proto`**: **`SessionEntry.pending_elicitation`** (field **14**).
- **`tddy_core`**: **`SessionMetadata.pending_elicitation`** in **`.session.yaml`** (serde default **`false`**).
- **`tddy-daemon`**: Module **`elicitation`** — list flag from metadata; **`TelegramSessionWatcher::on_server_message`** handles **`ModeChanged`** with dedupe and generic approval/input Telegram lines; **`session_list_enrichment`** sets the proto field. Tests: **`telegram_notifier`** acceptance unit tests, **`list_sessions_enriched`**, **`session_list_enrichment`** unit test.
- **Feature docs**: [telegram-notifications.md](telegram-notifications.md) (Presenter stream: elicitation); [web-terminal.md](../web/web-terminal.md) (pending elicitation on rows). Package: [telegram-notifier.md](../../packages/tddy-daemon/docs/telegram-notifier.md), [changesets.md](../../packages/tddy-daemon/docs/changesets.md). Cross-package: **[docs/dev/changesets.md](../../dev/changesets.md)**.

## 2026-04-05 — Documentation wrap (telegram presenter PRD retired)

- **Docs**: WIP PRD for Telegram **PresenterObserver** stream removed from **`docs/ft/daemon/1-WIP/`**; product and integration remain in [telegram-notifications.md](telegram-notifications.md). **`docs/dev/1-WIP/daemon-telegram-validate/`** report bundle removed. Cross-package note: **[docs/dev/changesets.md](../../dev/changesets.md)**.

## 2026-04-04 — Projects: `main_branch_ref` (git integration base)

- **Registry**: Optional **`main_branch_ref`** on project rows; **`effective_integration_base_ref_for_project`**; **`add_project`** rejects invalid refs before **`projects.yaml`** writes (**`tddy_core::validate_integration_base_ref`**).
- **Docs**: [git-integration-base-ref.md](../coder/git-integration-base-ref.md), [project-concept.md](project-concept.md); package [connection-service.md](../../packages/tddy-daemon/docs/connection-service.md).
- **PRD retired**: Prior WIP PRD for the multi-user daemon was merged into [project-concept.md](project-concept.md) (**Multi-user daemon**) and this changelog; source file removed from **`docs/ft/daemon/1-WIP/`**.

## 2026-04-04 — Worktrees library + ConnectionService RPCs

- **`tddy_daemon::worktrees`**: Parses **`git worktree list`** output; **`WorktreeStatsCache`** persists per-project snapshots under **`TDDY_PROJECTS_STATS_ROOT`** (default **`~/.tddy/projects`**); **`validate_worktree_path_within_repo_root`** (lexical containment); **`remove_worktree_under_repo`** (membership in **`git worktree list`**, refuses primary worktree).
- **ConnectionService**: **`ListWorktreesForProject`** (optional **`refresh`** → **`refresh_stats_for_project`** in **`spawn_blocking`**), **`RemoveWorktree`** (invalidates cache on success). Project path via **`main_repo_path_for_host`** and local **`daemon_instance_id`** (remote daemon routing for these RPCs is out of scope). Tests: **`worktrees`**, **`worktrees_acceptance`**, **`worktrees_rpc`** (requires **`git`**, **`USER`** for registry tests).
- **Package doc**: [worktrees.md](../../packages/tddy-daemon/docs/worktrees.md), [connection-service.md](../../packages/tddy-daemon/docs/connection-service.md). Web feature: [worktrees.md](../web/worktrees.md).

## 2026-04-03 — Telegram session notifications (library)

- **Config**: Optional **`telegram`** block in **`daemon.yaml`** with **`enabled`**, **`bot_token`**, and **`chat_ids`** (integer chat targets); unknown keys on the block are rejected under **`deny_unknown_fields`**.
- **Behavior**: The **`tddy_daemon::telegram_notifier`** module provides **`TelegramSessionWatcher`** (baseline + one notification per status transition for active sessions), **`session_telegram_label`** (first two hyphen segments of **`session_id`**), **`mask_bot_token_for_logs`**, and **`send_telegram_via_teloxide`** (teloxide **`Bot::send_message`**). Tests use a mock **`TelegramSender`**; CI avoids the live Telegram API.
- **Docs**: Product reference **[telegram-notifications.md](telegram-notifications.md)**; technical reference **[telegram-notifier.md](../../packages/tddy-daemon/docs/telegram-notifier.md)**.

## 2026-04-03 — ConnectionService: workflow files, session base path, delete

- **`ListSessionWorkflowFiles`** / **`ReadSessionWorkflowFile`**: Allowlisted basenames (`changeset.yaml`, `.session.yaml`, `PRD.md`, `TODO.md`) under **`{sessions_base}/sessions/{session_id}/`** with canonical-path checks (**`session_workflow_files`**; tests **`session_workflow_files_rpc`**).
- **Sessions base**: **`sessions_base_for_user`** resolves the Tddy **data directory** (typically **`~/.tddy`**), matching **`tddy_core::output::tddy_data_dir_path`** when **`TDDY_SESSIONS_DIR`** is unset, so listing/connect/delete target the same trees as **`tddy-coder`**.
- **`DeleteSession`**: Terminates a live **`metadata.pid`** when needed (SIGTERM/SIGKILL; Linux zombie handling), then removes the directory; directories without readable **`.session.yaml`** are removed when the resolved path is valid.
- **Package**: [connection-service.md](../../packages/tddy-daemon/docs/connection-service.md). Web: [web-terminal.md](../web/web-terminal.md), [web changelog](../web/changelog.md).

## 2026-03-29 — ConnectionService: `ListAgents` and `allowed_agents`

- **Config**: Daemon YAML includes **`allowed_agents`**, a list of **`id`** (required) and optional **`label`** entries (same shape as tool allowlist entries; unknown keys on each entry are rejected when using **`deny_unknown_fields`**).
- **`ListAgents`**: Returns **`AgentInfo`** rows in config order; display labels use trimmed non-empty **`label`**, otherwise **`id`**.
- **`StartSession`**: When **`allowed_agents`** is non-empty, a non-empty **`agent`** must match an **`id`**; otherwise **`INVALID_ARGUMENT`**. An empty **`allowed_agents`** list does not apply this check.
- **Implementation**: Shared mapping lives in **`agent_list_mapping`**; integration tests cover config parse, RPC payloads, **`ListTools`** regression, and unknown agent rejection.
- **Package doc**: [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md). **Install / config**: [systemd-install.md](systemd-install.md).

## 2026-03-28 — Unified session tree and `session_id` validation

- **Filesystem**: Session directories use `{sessions_base}/sessions/{session_id}/` consistently for listing, connect, resume, signal, delete, and headless `GetSession` / `ListSessions`.
- **Validation**: `session_id` is validated as a single safe path segment on **ConnectSession**, **ResumeSession**, **SignalSession**, **DeleteSession**, and service-side **GetSession** before paths are built (aligned with `session_deletion` rules).
- **Feature reference**: [Session directory layout](../coder/session-layout.md) ([migration from non-unified trees](../coder/session-layout.md#migration-from-non-unified-trees)).

## 2026-03-28 — StartSession and spawn: `recipe`

- **`StartSession` / `StartSessionRequest`**: Optional **`recipe`** (`tdd` or `bugfix`); empty behaves like **`tdd`**. Session **`changeset.yaml`** persists **`recipe`** for the new session.
- **Spawn**: **`SpawnRequest`** includes **`recipe`**; the daemon passes **`--recipe`** to **`tddy-coder`** when set.
- **Package**: [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md). Coder feature: [workflow-recipes.md](../coder/workflow-recipes.md).

## 2026-03-28 — ConnectionService: multi-host selection + ListSessions workflow enrichment

- **`ListEligibleDaemons`**: Returns eligible daemon entries from **`EligibleDaemonSource`** (local instance; LiveKit peer discovery deferred).
- **`ListSessions`**: Each **`SessionEntry`** includes **`daemon_instance_id`** for the owning daemon, plus **`workflow_goal`**, **`workflow_state`**, **`elapsed_display`**, **`agent`**, and **`model`** from **`.session.yaml`** / **`changeset.yaml`** via **`session_list_enrichment`**. Blocking read and enrichment run on the thread pool via **`spawn_blocking_with_timeout`**. Enrichment failures are logged at **warn**; the RPC still returns base session fields from **`session_reader`**.
- **`StartSession`**: Accepts optional **`daemon_instance_id`**; local spawn when empty or matching the local instance; non-local targets return **unimplemented** until cross-daemon spawn routing exists.
- **Proto / service**: **`connection.proto`** defines **`SessionEntry`** fields; TypeScript and Rust stubs are generated from the proto.
- **Package doc**: [connection-service.md](../../packages/tddy-daemon/docs/connection-service.md). Web UX: [web-terminal.md](../web/web-terminal.md).

## 2026-03-24 — ConnectionService: DeleteSession

- **`DeleteSession`**: Removes the on-disk session directory under the authenticated user’s **`{sessions_base}/sessions/{session_id}/`** tree. Rejects invalid session ids with **`INVALID_ARGUMENT`**. Filesystem removal errors return a generic **`INTERNAL`** message to clients; full error detail is logged on the server.
- **Current behavior** (terminate live processes, metadata-less directories, **`sessions_base`** resolution): see **2026-04-03 — ConnectionService: workflow files, session base path, delete** above.

## 2026-03-23 — Root `./install --systemd`

- **Installer**: Repo **`./install --systemd`** (optional **`--build`** runs **`./release`** first) copies **`tddy-daemon`**, **`tddy-coder`**, **`tddy-tools`** from **`target/release/`**; installs **`daemon.yaml`** from **`daemon.yaml.production`** only when missing; writes **`tddy-daemon.service`**; copies **tddy-web** **`dist`** when present; runs **`systemctl`** unless **`INSTALL_NO_SYSTEMCTL=1`**.
- **Paths**: Overridable via **`INSTALL_PREFIX`**, **`INSTALL_BIN_DIR`**, **`INSTALL_CONFIG_DIR`**, **`INSTALL_SYSTEMD_DIR`**, **`INSTALL_WEB_BUNDLE_DIR`**.
- **Docs**: Feature summary in **[systemd-install.md](systemd-install.md)**. Example unit: **[docs/dev/tddy-daemon.service.example](../../dev/tddy-daemon.service.example)**.

## 2026-03-22 — LiveKit: `livekit.common_room` for spawns

- When **`livekit.common_room`** is set (non-empty), daemon-spawned **`tddy-*`** processes receive **`--livekit-room`** set to that value so all sessions share one room; **`--livekit-identity`** remains **`daemon-{session_id}`** per session. If unset or whitespace-only, the room name is **`daemon-{session_id}`** as before.

## 2026-03-21 — StartSession: `agent`

- **ConnectionService**: `StartSessionRequest` includes optional `agent`; forwarded to spawned `tddy-coder` as `--agent` when non-empty (skips interactive backend menu in the child).

## 2026-03-21 — Project concept

- **Projects**: Named `git_url` + `main_repo_path` per user; `~/.tddy/projects/projects.yaml`.
- **Config**: `repos_base_path` (default `repos` under user home).
- **ConnectionService**: `ListProjects`, `CreateProject` (optional `user_relative_path` for clone/adopt location under `~`); `StartSession` uses `project_id`; `SessionEntry` includes `project_id`.
- **Clone**: On create, clone into `{repos_base}/{name}/` unless path exists (then adopt).
- **Spawn**: `tddy-coder` receives `--project-id`; `.session.yaml` stores `project_id`.
- **PRD reference:** PRD-2026-03-21-project-concept.md (wrapped into [project-concept.md](project-concept.md)).
