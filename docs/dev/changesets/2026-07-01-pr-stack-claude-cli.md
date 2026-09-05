# 2026-07-01 — pr-stack-claude-cli

**Type:** Feature

PR stack parent picker for Claude CLI sessions; `recipe` field on proto `SessionEntry` (field 22) enables orchestrator-only filtering via `prStackOrchestrators()`; `start_claude_cli_session` and `start_sandboxed_claude_cli_session` set `orchestrator_session_id` and resolve git-base chain ref via `resolve_chain_integration_base_ref_from_parent_session`. Feature [session-drawer.md](../../ft/web/session-drawer.md). (tddy-service, tddy-daemon, tddy-web)
