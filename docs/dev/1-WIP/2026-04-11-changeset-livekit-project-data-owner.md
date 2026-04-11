# Changeset: LiveKit project-data owner (daemon library)

**Date:** 2026-04-11  
**Status:** Complete (documentation wrap)  
**Type:** Feature

## Affected packages

- **`tddy-daemon`**
- **`tddy-livekit`**
- **`tddy-coder`**
- **`docs`** (feature + dev indexes)

## Related feature documentation

- [docs/ft/daemon/livekit-project-data-ownership.md](../../ft/daemon/livekit-project-data-ownership.md)

## Summary

Single-writer semantics for the per-user **`projects.yaml`** registry when multiple daemons share a LiveKit **`common_room`**: YAML flag **`livekit.project_data_owner_eligible`**, env **`TDDY_LIVEKIT_PROJECT_DATA_OWNER`**, participant metadata with schema version, deterministic lexicographic election among eligible identities, and filesystem snapshot helpers. Integration tests use **`tddy-livekit-testkit`**.

## Technical state (documentation)

- **`DaemonConfig`**: **`apply_livekit_env_overrides`**, **`effective_project_data_owner_eligible`**.
- **`project_data_ownership`**: metadata build/parse, **`refresh_project_data_ownership_metadata`**, join helper, snapshot/converge helpers.
- **`ConnectionService`**: RPC catalog for projects unchanged; coordination described in feature doc.
- **Cross-package**: **`run_with_reconnect_metadata`** accepts **`codex_oauth_watch`**; **`tddy-coder`** passes it through.

## Acceptance tests (reference)

- **`config_project_ownership_yaml_and_env_effective_value`**
- **`livekit_metadata_contains_project_owner_fields_for_elected_daemon`**
- **`replica_project_registry_matches_owner_after_create`**
- **`dual_owner_eligibility_converges_to_single_writer`**
- Unit tests in **`project_data_ownership`**

## Production readiness notes

Process entrypoint applies Telegram env overrides; **`apply_livekit_env_overrides`** exists on **`DaemonConfig`** for integration during config load. LiveKit transport for incremental registry sync and replica **`ConnectionService`** policy are out of scope for this library drop.

## References

- [packages/tddy-daemon/docs/project-data-ownership.md](../../../packages/tddy-daemon/docs/project-data-ownership.md)
- [docs/ft/daemon/changelog.md](../../ft/daemon/changelog.md)
