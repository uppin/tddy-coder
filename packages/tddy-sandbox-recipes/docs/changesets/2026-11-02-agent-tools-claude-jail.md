# 2026-11-02 — Lowering of home, files, secrets and policy; persistent homes

**Type:** Feature

`tool_grants::{lower_home, lower_files, lower_secrets, lower_policy}` (`JailPaths`, `HomeGrant`, `FileGrants`) and `homes::{persistent_home, legacy_home, HomeError, HOMES_DIR}`: `<tddyhome>/homes/<key>`, renamed once from `sandbox-<key>-home`, refused when both exist. Gains `thiserror`. Cross-package: [agent-tools-claude-jail](../../../../docs/dev/changesets/2026-11-02-agent-tools-claude-jail.md).
