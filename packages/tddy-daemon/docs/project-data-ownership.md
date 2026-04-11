# Project-data ownership (`tddy-daemon`)

Technical reference for **`tddy_daemon::project_data_ownership`** and related **`DaemonConfig`** LiveKit fields.

## Module

**Path:** **`packages/tddy-daemon/src/project_data_ownership.rs`** (exported from **`lib.rs`**).

## Configuration (`config.rs`)

- **`LiveKitConfig::project_data_owner_eligible`** — serde default **`true`** when the field is omitted.
- **`DaemonConfig::apply_livekit_env_overrides`** — reads **`TDDY_LIVEKIT_PROJECT_DATA_OWNER`**; merges into **`livekit.project_data_owner_eligible`**; may insert **`LiveKitConfig::default()`** when **`livekit`** was **`None`**.
- **`DaemonConfig::effective_project_data_owner_eligible`** — **`true`** if **`livekit`** is **`None`**; otherwise the boolean on the livekit block.

## Public API (selection)

| Item | Purpose |
|------|---------|
| **`PROJECT_METADATA_SCHEMA_VERSION_KEY`** | JSON key for schema version in metadata. |
| **`PROJECT_DATA_METADATA_SCHEMA_VERSION`** | Current version constant (**`1`**). |
| **`LIVEKIT_IDENTITY_PROJECT_DATA_INELIGIBLE`** | Identity string excluded from election (**`daemon-ineligible`**). |
| **`build_project_data_participant_metadata`** | Serialize owner JSON. |
| **`parse_project_data_participant_metadata`** / **`metadata_claims_active_project_data_owner`** | Parse and validate owner claims. |
| **`livekit_identity_eligible_for_project_data_ownership`** | Eligibility predicate for election set. |
| **`elect_project_data_owner`** | Lexicographic minimum over non-empty candidate strings. |
| **`refresh_project_data_ownership_metadata`** | Recompute election from **`Room`** local + remote participants; **`LocalParticipant::set_metadata`**. |
| **`join_common_room_and_publish_project_ownership_metadata`** | **`Room::connect`** + initial **`refresh_project_data_ownership_metadata`** (warns on failure, still returns room). |
| **`apply_owner_project_registry_snapshot_to_replica`** | **`read_projects`** / **`write_projects`** from owner dir to replica dir. |
| **`converge_replica_project_registry_with_elected_owner`** | Chooses authoritative directory by **`Path`** string order (**`to_string_lossy()`**); copies snapshot to the other. |
| **`count_remote_project_data_owners`** / **`project_data_owner_flag_from_metadata`** | Test and observer helpers. |

## Dependencies

The crate depends on **`livekit`** (**`0.7`**) for **`Room`**, **`RoomEvent`**, and **`LocalParticipant`**, in addition to **`tddy-livekit`** used elsewhere.

## Logging

Uses **`log::debug!`** and **`log::info!`** with targets **`tddy_daemon::project_data_ownership`** and **`tddy_daemon::config`** (see implementation for message shapes).

## Tests

- **Unit:** **`project_data_ownership_unit_tests`** in the same module; **`project_data_ownership_env_tests`** in **`config.rs`**.
- **Integration:** **`tests/livekit_project_ownership.rs`** — requires **`tddy-livekit-testkit`** (container or **`LIVEKIT_TESTKIT_WS_URL`**).

## Product context

Feature requirements and operator-facing behavior: **[docs/ft/daemon/livekit-project-data-ownership.md](../../../docs/ft/daemon/livekit-project-data-ownership.md)**.
