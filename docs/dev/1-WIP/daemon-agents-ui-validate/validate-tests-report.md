# Validate Tests Report

## Context

This run validates automated tests for the daemon-agents / ListAgents work described in [evaluation-report.md](./evaluation-report.md) (allowed agents YAML, `ListAgents` RPC, web wiring, and related tests).

## Commands executed

1. **Workspace Rust tests (verify script)** — from repo root:

   ```bash
   ./verify
   ```

   Implementation (from `verify`): `cargo build -p tddy-acp-stub` then `cargo test -- --test-threads=1` with output tee’d to `.verify-result.txt`. Completed with **exit code 0** in approximately **186 seconds** (~3.1 minutes).

2. **Bun unit tests (agent options)** — from `packages/tddy-web`:

   ```bash
   cd packages/tddy-web && bun test src/components/connection/agentOptions.test.ts
   ```

   Completed with **exit code 0**.

## Results summary

### `./verify` (full workspace `cargo test`)

- **Outcome:** All Rust test suites reported **`test result: ok`** with **`0 failed`**; `.verify-result.txt` contains **no** `FAILED` or `failures:` lines.
- **ListAgents–related packages (factual excerpts from `.verify-result.txt`):**
  - **`tddy-daemon` library tests:** 33 passed, 0 failed (includes `agent_list_mapping` unit tests: `agent_allowlist_rows_blank_trimmed_label_falls_back_to_id`, `agent_allowlist_rows_match_list_agents_label_rules`).
  - **`list_agents_allowlist_acceptance` integration:** 4 passed, 0 failed — `daemon_config_allowed_agents_deserializes`, `connection_service_list_agents_returns_config`, `list_tools_unchanged_with_new_config_field`, `start_session_unknown_agent_rejected`.

### `bun test src/components/connection/agentOptions.test.ts`

- **Outcome:** **3 passed**, **0 failed** (`buildAgentSelectOptionsFromRpc` mapping; two `coalesceBackendAgentSelection` cases).

### Key failures

- **None** for the commands above.

## Coverage gaps / recommendations

Compared to a typical PRD for **ListAgents** and connection UI, the following are **not exercised by the two commands run here** (some may exist elsewhere in the repo):

- **Cypress component / e2e:** `./verify` does not run `bun run cypress:component` or `cypress:e2e`. [evaluation-report.md](./evaluation-report.md) notes `ConnectionScreen.cy.tsx` updates for ListAgents stubs and backend select behavior; those were **not** re-run in this validate pass.
- **End-to-end full stack:** No automated step here starts a live daemon plus Vite/web and asserts ListAgents over real Connect-RPC; coverage is unit + integration + (separately documented) component tests.
- **Negative UI paths:** Dedicated CT/e2e for ListAgents RPC failure, timeout, or mismatch with an older daemon (evaluation flags `Promise.all` coupling with ListTools) would strengthen regression safety beyond Rust acceptance tests.
- **Empty allowlist UX:** Rust integration covers config deserialization and allowlist echo; explicit UI/CT for **zero agents** (empty dropdown, messaging, start disabled) is a common PRD item worth confirming in Cypress or manual QA.
- **Operator documentation:** As in the evaluation report, operator-facing `allowed_agents` docs are expected via the docs changeset workflow, not test commands.

---

File written: `/var/tddy/Code/tddy-coder/.worktrees/feat-daemon-agents-ui/docs/dev/1-WIP/daemon-agents-ui-validate/validate-tests-report.md`
