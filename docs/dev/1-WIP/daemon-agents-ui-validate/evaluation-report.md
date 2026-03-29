# Evaluation Report

## Summary

The worktree implements daemon-backed allowed agents end-to-end: YAML config, ListAgents RPC, shared agent_list_mapping for labels, ConnectionScreen fetching agents with tools, unit and integration tests, and Cypress coverage including default ListAgents stubs so existing CTs keep working. cargo check for tddy-daemon and tddy-service succeeded. Notable gaps: operator documentation per PRD is not in the diff; temporary debug logging (console.debug, log::info per agent) remains for later cleanup; untracked .tddy-red-* artifacts should not be committed; Promise.all couples ListTools and ListAgents so either RPC failure clears both lists and surfaces one error—stricter than prior ListTools-only behavior.

## Risk Level

medium

## Changed Files

- dev.daemon.yaml (modified, +11/−0)
- packages/tddy-daemon/src/config.rs (modified, +17/−0)
- packages/tddy-daemon/src/connection_service.rs (modified, +44/−6)
- packages/tddy-daemon/src/lib.rs (modified, +1/−0)
- packages/tddy-daemon/src/agent_list_mapping.rs (added, +85/−0)
- packages/tddy-daemon/tests/list_agents_allowlist_acceptance.rs (added, +184/−0)
- packages/tddy-service/proto/connection.proto (modified, +10/−0)
- packages/tddy-web/cypress/component/ConnectionScreen.cy.tsx (modified, +146/−1)
- packages/tddy-web/src/components/ConnectionScreen.tsx (modified, +62/−18)
- packages/tddy-web/src/components/connection/agentOptions.ts (added, +43/−0)
- packages/tddy-web/src/components/connection/agentOptions.test.ts (added, +33/−0)
- packages/tddy-web/src/gen/connection_pb.ts (modified, +82/−22)

## Affected Tests

- packages/tddy-daemon/src/agent_list_mapping.rs: created — Unit tests: label rules and blank-label fallback.
- packages/tddy-daemon/tests/list_agents_allowlist_acceptance.rs: created — Integration: config deserialize, ListAgents echo, ListTools regression, unknown agent on StartSession.
- packages/tddy-web/src/components/connection/agentOptions.test.ts: created — Bun unit tests for option mapping and coalesce selection.
- packages/tddy-web/cypress/component/ConnectionScreen.cy.tsx: updated — ListAgents intercepts (default + custom), connection_screen_backend_select_uses_list_agents; Terminate-cancel stability waits.

## Validity Assessment

Yes. The diff matches the PRD intent: server-side allowlist in config, Connection API extension, daemon implementation without hardcoded agent lists for ListAgents, web UI driven by RPC with coalescing similar to tools, dev.daemon.yaml defaults preserving the four backends, and automated tests at unit, integration, and component levels. Remaining work is mainly documentation workflow and production-hardening (log levels, deploy/version notes), not functional gaps visible in the code review.

## Build Results

- tddy-daemon+tddy-service: pass (./dev cargo check -p tddy-daemon -p tddy-service exited 0 (~32s). tddy-web is TypeScript/Bun—not compiled by cargo.)

## Issues

- [low/hygiene] .tddy-red-capture.txt: Untracked workflow artifact in repo root; risk of accidental commit or noise in reviews. Suggestion: Delete or gitignore before merge.
- [low/hygiene] .tddy-red-submit.json: Untracked workflow artifact in repo root. Suggestion: Delete or gitignore before merge.
- [low/maintainability] packages/tddy-web/cypress/component/ConnectionScreen.cy.tsx: MOCK_DEFAULT_LIST_AGENTS duplicates dev.daemon.yaml defaults; drift possible if one side changes. Suggestion: Document linkage or generate from a single fixture in a later refactor.
- [medium/compatibility] packages/tddy-web/src/components/ConnectionScreen.tsx: UI now requires ListAgents; older daemons without the RPC will fail the combined load with ListTools (Promise.all). Suggestion: Ensure daemon and web ship together; document minimum daemon version if applicable.
- [low/observability] packages/tddy-daemon/src/agent_list_mapping.rs: log::info per allowlist row may be verbose at default log levels in production. Suggestion: Downgrade to debug-only in refactor phase as planned.
- [low/documentation] docs/: PRD asks for operator-facing docs for allowed_agents; not present in this diff (expected via changeset workflow). Suggestion: Follow docs/dev workflow before release.
