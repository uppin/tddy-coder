# 2026-10-23 — EnsureProjectHomeSession and SessionEntry.session_role
**Type:** Feature

`session.proto`: `SessionEntry.session_role` (field 35) and `SessionService.EnsureProjectHomeSession` (`project_id`, `daemon_instance_id` → `session`, `outcome` = started / resumed / running); the RPC is registered in `unbundle_service_split`'s residual methods (#699, `#project-home` 4/7).
