# 2026-06-26 — **PR-stack session UI

**Type:** Feature

proto plumbing + stack_parent spawn threading** — `connection.proto`: `orchestrator_session_id = 21` on `SessionEntry`, `stack_parent = 15` on `StartSessionRequest`; `session_list_enrichment.rs`: `orchestrator_session_id` on `SessionListStatusDisplay`, read from `changeset.orchestrator_session_id`, applied to proto in `apply_session_list_status_to_proto`; `spawner.rs`: `stack_parent: Option<&str>` on `SpawnOptions`, passes `--stack-parent` arg to child; `spawn_worker.rs`: `stack_parent` field on `SpawnRequest` (fixes silent drop in worker path), threaded through `build_spawn_request` + `spawn_worker_main`; `connection_service.rs`: `stack_parent_for_spawn` derived from request, passed to both direct+worker `SpawnOptions`. Tests: 3 enrichment unit tests. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-daemon)
