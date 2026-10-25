# 2026-10-24 — A coding-agent session over either transport, and the approval routes

**Type:** Feature

`#coagents` 7/8 ([#726](https://github.com/uppin/tddy-coder/pull/726)). `CodingAgentSession` holds a `dyn ConversationDriver`; `start_headless` opens one over `HeadlessDriver`, and `permissions()` exposes its `ParkedPermissions`. New `transports` (`transport_of`, `permission_policy_of`, `transport_and_policy_wire`, `HEADLESS_HARNESSES`) and `approval_routes` (`ApprovalRoutes`: launch binding by session and minted credential, constant-time match, 60 s bound, a drop guard that cancels only its own request). `ParkedPermissions::cancel_request`. The launcher port gains `headless_command_for` (default: refused), `SESSION_TOKEN_ENV`, `HEADLESS_DIR`, `headless_conversation_dir`. The catalog offers profiles of headless harnesses; a bare `allowed_agents` id stays ACP-only.

Code issues: `oversized-file-session` 530 → 575, deferred to the sweep node. See [architecture](../architecture.md).
