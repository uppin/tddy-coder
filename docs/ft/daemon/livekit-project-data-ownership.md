# LiveKit project-data ownership

**Status:** Current  
**Scope:** Per-user project registry (`~/.tddy/projects/projects.yaml`) when multiple **`tddy-daemon`** processes share a LiveKit **`common_room`**.

## Summary

At most one daemon in that room acts as the **project-data owner**: the sole participant that may treat the local **`projects.yaml`** as authoritative for networked consistency. Other daemons are **replicas**; they observe ownership through LiveKit participant metadata and align local registry state using library helpers (snapshot copy / convergence). Eligibility to **win** election is configurable in YAML and overridable from the process environment when **`DaemonConfig::apply_livekit_env_overrides`** runs.

## Configuration

Under **`livekit:`** in daemon YAML:

| Field | Type | Default | Meaning |
|--------|------|---------|---------|
| **`project_data_owner_eligible`** | bool | `true` | When `true`, this process may become the active project-data owner in **`common_room`** (election among eligible peers). |

Environment variable **`TDDY_LIVEKIT_PROJECT_DATA_OWNER`** (`true` / `false` / `1` / `0` / `yes` / `no` / `on` / `off`, case-insensitive) sets **`livekit.project_data_owner_eligible`** when present. If no **`livekit:`** block exists, **`apply_livekit_env_overrides`** creates a default block so the flag applies. Invalid values produce a warning log and leave the field unchanged.

**`DaemonConfig::effective_project_data_owner_eligible`**: when **`livekit`** is absent from config, the effective value is **`true`** (single-daemon / local operation).

Sample comments appear in repo **`dev.daemon.yaml`** under **`livekit:`**.

## Participant metadata

The active owner publishes JSON on the local LiveKit participant including:

- **`daemon_instance_id`** — populated from the local participant identity string in the library join path (operators should align LiveKit identity with multi-host instance identity when integrating).
- **`project_data_owner`** — boolean; **`true`** only for the elected eligible participant.
- **`project_metadata_schema_version`** — unsigned integer (**`1`** in the current schema) for forward compatibility.

Election among **eligible** identities uses the **lexicographically smallest** non-empty participant identity. The identity **`daemon-ineligible`** is excluded from election (acceptance-test and harness convention for “ownership disabled”).

## Library surface (`tddy-daemon`)

| Area | Role |
|------|------|
| **`project_data_ownership`** | Build/parse metadata, **`elect_project_data_owner`**, **`refresh_project_data_ownership_metadata`**, **`join_common_room_and_publish_project_ownership_metadata`**, **`apply_owner_project_registry_snapshot_to_replica`**, **`converge_replica_project_registry_with_elected_owner`**. |
| **Tests** | Unit tests in **`project_data_ownership.rs`**; integration tests in **`tests/livekit_project_ownership.rs`** (**`tddy-livekit-testkit`**, optional **`LIVEKIT_TESTKIT_WS_URL`**). |

## ConnectionService and runtime wiring

**`ListProjects`**, **`CreateProject`**, and session spawn paths use **`project_storage`** as implemented; the library module does **not** replace or intercept those RPCs. LiveKit-backed incremental sync and explicit replica RPC policy (forward, cached read, or error) remain integration work for the daemon process entrypoint and **`ConnectionService`**.

The **`tddy-daemon`** process entrypoint applies **`apply_telegram_env_overrides`** after loading YAML; **`apply_livekit_env_overrides`** is available on **`DaemonConfig`** for callers that invoke it during config load.

## Related

- [Project concept](project-concept.md) — registry location and **`CreateProject`** / **`ListProjects`** semantics.
- Package technical reference: [project-data-ownership.md](../../packages/tddy-daemon/docs/project-data-ownership.md).
- [Connection service](../../packages/tddy-daemon/docs/connection-service.md) — RPC catalog.
