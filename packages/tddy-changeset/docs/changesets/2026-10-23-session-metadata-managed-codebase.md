# 2026-10-23 — SessionMetadata.managed_codebase

**Type:** Feature

`#project-home` 3/7 (#698). `SessionMetadata.managed_codebase: bool`, `#[serde(default, skip_serializing_if = "is_false")]`: a missing key reads `false` and `false` is never written, so older `.session.yaml` files read unchanged (`a_session_yaml_without_the_key_reads_as_not_managed`).
