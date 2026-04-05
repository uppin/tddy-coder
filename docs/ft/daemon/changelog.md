# Daemon product area changelog

## 2026-04-05 — Telegram: inbound session control, PresenterIntent, elicitation UX

- **Inbound control**: Daemon runs **`telegram_bot`** (teloxide long-polling) when Telegram is configured and **`sessions_base`** resolves. Commands include **`/start-workflow`**, **`/sessions`**, **`/delete`**, **`/submit-feature`**, **`/answer-text`**, **`/answer-multi`**; callbacks cover session list, recipe/project/agent picks, document review (**`doc:`**), and elicitation select (**`eli:s:`**). **`TelegramSessionControlHarness`** and integration tests exercise the library; production uses **`TeloxideSender`** with the same bot as outbound notifications.
- **PresenterIntent**: **`presenter_intent.proto`** and **`tddy-daemon::presenter_intent_client`** forward answers and document actions to the child **`tddy-coder`** on localhost gRPC.
- **Outbound notifications**: **`ModeChanged`** for document review / markdown viewer sends **full document body** (chunked), then **Approve** / **Reject** / **Refine** (and related) inline actions. **`Select`** clarification sends a **numbered option list** in the message body, **numeric** inline buttons, and a **post-tap confirmation** with the full chosen option text. Dedupe for identical **`ModeChanged`** payloads per session is unchanged.
- **Formatting**: Styled text must follow Telegram **[message entities](https://core.telegram.org/api/entities)** rules (UTF-16 code units for offsets and lengths where applicable).
- **Feature docs**: [telegram-session-control.md](telegram-session-control.md), [telegram-notifications.md](telegram-notifications.md).

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

- **`DeleteSession`**: Removes the on-disk session directory under the authenticated user’s sessions base when `.session.yaml` indicates an **inactive** session (no live process for the recorded PID, consistent with `ListSessions`). Rejects active sessions, missing sessions, and invalid session ids with the appropriate gRPC status. Filesystem removal errors return a generic **`INTERNAL`** message to clients; full error detail is logged on the server.

## 2026-03-23 — Root `./install --systemd`

- **Installer**: Repo **`./install --systemd`** (optional **`--build`** runs **`./release`** first) copies **`tddy-daemon`**, **`tddy-coder`**, **`tddy-tools`** from **`target/release/`**; installs **`daemon.yaml`** from **`daemon.yaml.production`** only when missing; writes **`tddy-daemon.service`**; copies **tddy-web** **`dist`** when present; runs **`systemctl`** unless **`INSTALL_NO_SYSTEMCTL=1`**.
- **Paths**: Overridable via **`INSTALL_PREFIX`**, **`INSTALL_BIN_DIR`**, **`INSTALL_CONFIG_DIR`**, **`INSTALL_SYSTEMD_DIR`**, **`INSTALL_WEB_BUNDLE_DIR`**.
- **Docs**: Feature summary in **[systemd-install.md](systemd-install.md)**. Example unit: **[docs/dev/tddy-daemon.service.example](../../dev/tddy-daemon.service.example)**.

## 2026-03-22 — LiveKit: `livekit.common_room` for spawns

- When **`livekit.common_room`** is set (non-empty), daemon-spawned **`tddy-*`** processes receive **`--livekit-room`** set to that value so all sessions share one room; **`--livekit-identity`** remains **`daemon-{session_id}`** per session. If unset or whitespace-only, the room name is **`daemon-{session_id}`** as before.

## 2026-03-21 — StartSession: `agent`

- **ConnectionService**: `StartSessionRequest` includes optional `agent`; forwarded to spawned `tddy-coder` as `--agent` when non-empty (skips interactive backend menu in the child).

## 2026-03-21 — PRD: implementation status

- **[PRD: tddy-daemon](1-WIP/PRD-2026-03-19-tddy-daemon.md)** updated with **Implementation status (2026-03-21)**: Phase 1 (binary, OAuth, spawn, project-centric web UX) documented; full success-criteria checklist remains open for validation.

## 2026-03-21 — Project concept

- **Projects**: Named `git_url` + `main_repo_path` per user; `~/.tddy/projects/projects.yaml`.
- **Config**: `repos_base_path` (default `repos` under user home).
- **ConnectionService**: `ListProjects`, `CreateProject` (optional `user_relative_path` for clone/adopt location under `~`); `StartSession` uses `project_id`; `SessionEntry` includes `project_id`.
- **Clone**: On create, clone into `{repos_base}/{name}/` unless path exists (then adopt).
- **Spawn**: `tddy-coder` receives `--project-id`; `.session.yaml` stores `project_id`.
- **PRD reference:** PRD-2026-03-21-project-concept.md (wrapped into [project-concept.md](project-concept.md)).
