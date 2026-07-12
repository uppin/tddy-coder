# Participant metadata (LiveKit)

## Scope

**`tddy-livekit`** composes JSON strings for **`LocalParticipant::set_metadata`** on server participants: **Codex OAuth** file polling, optional **project registry** row counts, optional **`session`** presence from **`tddy-coder`**, and optional **`watch::Receiver<String>`** payloads from **`tddy-coder`**. All paths produce **one merged object** per write so top-level keys are not dropped between publishers.

## Public API

- **`OWNED_PROJECT_COUNT_METADATA_KEY`**: canonical string **`owned_project_count`** (re-exported from **`participant`**).
- **`merge_participant_metadata_json(baseline, update)`**: shallow merge of two JSON objects; non-object baseline is treated as **`{}`** with a warning log.
- **`owned_project_count_for_projects_dir(path)`**: returns the number of rows in **`path/projects.yaml`** using the same schema as **`tddy_daemon::project_storage`** (implemented in **`src/projects_registry.rs`**; **tddy-livekit** does not depend on **tddy-daemon** to avoid a crate cycle — keep **`ProjectData`** fields aligned with the daemon when the schema evolves).
- **`spawn_local_participant_metadata_watcher(rx, local, metadata_publish_lock)`**: on each watch message, merges into **`local.metadata()`** under the lock, then **`set_metadata`**.
- **`LiveKitParticipant::metadata_publish_lock()`**: **`Arc<tokio::sync::Mutex<()>>`** shared with internal OAuth and registry tasks; **tddy-coder** passes this into the watcher after **`connect`**.

## `session` metadata key

**`tddy-coder`**'s participant publishes a **`session`** object (sibling of **`owned_project_count`** / **`codex_oauth`**) on workflow-state transitions, shallow-merged via **`merge_participant_metadata_json`** so all keys coexist. Schema:

| Field | Type | Notes |
|-------|------|-------|
| `session_id` | string | the session the block describes (matches the identity suffix `daemon-{instanceId}-{sessionId}`) |
| `workflow_goal` | string | the PRD goal line (empty until the first plan transition) |
| `workflow_state` | string | current `WorkflowState` label (e.g. `idle`, `planning`, `coding`, `verifying`) |
| `elapsed_display` | string | compact elapsed string (same rules as the TUI status bar / `format_elapsed_compact`) |
| `agent` | string | active agent / backend name (e.g. `claude`, `codex`) |
| `model` | string | active model identifier |
| `activity_status` | string | activity status label |
| `recipe` | string | workflow recipe (e.g. `tdd`) |
| `repo_path` | string | session worktree / repo path |
| `pending_elicitation` | bool | true when the workflow blocks on operator input |

The key is **tolerated as absent** on older participants; consumers treat missing/empty/invalid JSON as no overlay. Field shapes mirror the daemon's **`SessionListStatusDisplay`** enrichment so the web renders them identically whether sourced from presence or from `ListSessions`.

## `LiveKitParticipant` wiring

- **`connect`** / **`connect_for_reconnect`**: last options include **`codex_oauth_watch`** and **`projects_registry_dir`**. A fresh **`metadata_publish_lock`** is created per connected participant instance.
- **OAuth poller** (when **`codex_oauth_watch`** is **`Some`**): reads the hook file, builds a **`codex_oauth`** fragment, merges with current wire metadata, **`set_metadata`**.
- **Registry poller** (when **`projects_registry_dir`** is **`Some`**): applies **`owned_project_count`** immediately, then every **30 seconds** (bounded polling; file notify is a possible future replacement).

## Logging

Structured messages use targets such as **`tddy_livekit::metadata`**, **`tddy_livekit::codex_oauth`**, **`tddy_livekit::projects_registry`**. **`log::debug!`** / **`log::info!`** / **`log::warn!`** cover merge inputs, publish success, and parse failures.

## Tests

- **`tests/participant_metadata_acceptance.rs`**: merge preserves **OAuth** + count; LiveKit harness observes remote **`owned_project_count`** against a temp **`projects.yaml`**.
- **`tests/participant_metadata_unit.rs`**: row count matches file contents; the **`session`** key is preserved across merges with **`owned_project_count`** / **`codex_oauth`**.
- **Unit test** in **`participant.rs`**: merge retains baseline-only keys when the update adds **`owned_project_count`**.

## Related feature documentation

- **[LiveKit common room: owned project count](../../../../docs/ft/web/livekit-participant-owned-projects.md)** — `owned_project_count` + `session` keys.
- **[Session Participant RPC & Metadata](../../../../docs/ft/coder/session-participant-rpc.md)** — `tddy-coder` publisher of the `session` block.
