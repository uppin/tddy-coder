# 2026-07-01 — PR stack support for Claude CLI sessions

**Type:** Feature

`session_list_enrichment` surfaces `recipe` from `changeset.yaml` into `SessionListStatusDisplay` and proto `SessionEntry` field 22; `start_claude_cli_session` + `start_sandboxed_claude_cli_session` accept `stack_parent` — sets `orchestrator_session_id` on child changeset and resolves git chain base via `resolve_chain_integration_base_ref_from_parent_session`; `resolve_chain_base_ref` private helper deduplicates both paths. Feature [session-drawer.md](../../../../docs/ft/web/session-drawer.md). (tddy-service, tddy-daemon, tddy-web)
