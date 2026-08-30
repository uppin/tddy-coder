# Participant metadata (LiveKit)

## Scope

**`tddy-livekit`** composes JSON strings for **`LocalParticipant::set_metadata`** on server participants: **Codex OAuth** file polling, optional **project registry** row counts, optional **`session`** presence, and optional **`watch::Receiver<String>`** payloads. All paths produce **one merged object** per write so top-level keys are not dropped between publishers.

The **`session`** block has **two** publishers — **`tddy-coder`** for the sessions it runs, and **`tddy-daemon`** for a **`claude-cli`** session, whose bridge previously published nothing. Because the merge is **shallow**, a partial **`session`** object from either would *replace* the other's rather than merge with it, so the block's shape is owned by one type, **`tddy_core::session_participant_metadata::SessionParticipantMetadata`**, and every key is always emitted — empty where the publisher has nothing to say.

## Public API

- **`OWNED_PROJECT_COUNT_METADATA_KEY`**: canonical string **`owned_project_count`** (re-exported from **`participant`**).
- **`merge_participant_metadata_json(baseline, update)`**: shallow merge of two JSON objects; non-object baseline is treated as **`{}`** with a warning log.
- **`owned_project_count_for_projects_dir(path)`**: returns the number of rows in **`path/projects.yaml`** using the same schema as **`tddy_daemon::project_storage`** (implemented in **`src/projects_registry.rs`**; **tddy-livekit** does not depend on **tddy-daemon** to avoid a crate cycle — keep **`ProjectData`** fields aligned with the daemon when the schema evolves).
- **`spawn_local_participant_metadata_watcher(rx, local, metadata_publish_lock)`**: on each watch message, merges into **`local.metadata()`** under the lock, then **`set_metadata`**.
- **`LiveKitParticipant::metadata_publish_lock()`**: **`Arc<tokio::sync::Mutex<()>>`** shared with internal OAuth and registry tasks; **tddy-coder** passes this into the watcher after **`connect`**.

## `session` metadata key

A participant publishes a **`session`** object (sibling of **`owned_project_count`** / **`codex_oauth`**), shallow-merged via **`merge_participant_metadata_json`** so all keys coexist. **`tddy-coder`** republishes it on every workflow-state transition; **`tddy-daemon`** publishes it for a **`claude-cli`** session at spawn and re-sends it unchanged every **30 seconds**, since that session has no transition stream to ride and a single failed **`set_metadata`** would otherwise cost it the block for its whole life. Schema:

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
| `orchestrator_session_id` | string | the pr-stack orchestrator that spawned this session; empty when it is nobody's stack child |
| `stack_node_id` | string | the planned node this session materializes; empty is **no association**, never a wildcard |
| `branch` | string | the branch this session created — the branch that exists, not a planned name |

The key is **tolerated as absent** on older participants; consumers treat missing/empty/invalid JSON as no overlay. Field shapes mirror the daemon's **`SessionListStatusDisplay`** enrichment so the web renders them identically whether sourced from presence or from `ListSessions`.

The last three fields are the session's **stack association**, and presence is the only signal carrying them across a host boundary: `ListSessions` answers for one daemon's own sessions tree and does not fan out. They are what lets the PR-Stack view join a child running on another host back to the planned PR it is working — see [PR-Stack live status § Cross-host planned PRs](../../../../docs/ft/coder/pr-stack-live-status.md). `stack_node_id` exists **only** here: it is deliberately not a `SessionEntry` field, because it is needed exactly where a participant is live.

A daemon-published block carries the identity, the association and the static fields (`agent`, `model`, `recipe`, `repo_path`); the live workflow fields stay empty for such a session. A **sandboxed** claude-cli session joins no LiveKit room at all, so it publishes no block.

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
