# Clean Code Analysis Report

## Summary

The daemon-agents feature is structurally sound: allowlist mapping lives in a small Rust module, the service delegates `list_agents` to it, and the web layer isolates RPC-to-select logic in `agentOptions.ts` with Bun tests. Main clean-code gaps are **cross-boundary duplication** (Cypress `MOCK_DEFAULT_LIST_AGENTS` vs `dev.daemon.yaml`, repeated intercept factories), **mixed vocabulary** (backend vs agent), **temporary / verbose instrumentation** (`console.debug`, `log::info` per row), and **some UI duplication** (rebuilding agent options in several places). Cypress helpers trade DRY for readability in places but copy large RPC-setup blocks three times.

## What works well

- **Single source for server label rules**: `agent_allowlist_rows` in `agent_list_mapping.rs` is documented as matching `ConnectionServiceImpl::list_agents`, with focused unit tests for trim/fallback behavior. `list_agents` stays thin: map rows to `AgentInfo` only.

- **Separation on the web**: `agentOptions.ts` holds pure, testable helpers (`buildAgentSelectOptionsFromRpc`, `coalesceBackendAgentSelection`) instead of embedding option-shaping inside the screen component.

- **Validation at the boundary**: `start_session` enforces `allowed_agents` when a non-empty agent is supplied, keeping policy next to session creation rather than only in the UI.

- **Intercept helpers**: `mockListAgentsResponse` and `interceptAllRpcsWithListAgents` are named clearly; the ListAgents-specific test documents intent in a file-level comment.

- **PRD traceability**: Module-level and field comments tie behavior to the PRD where it matters (`agent_list_mapping`, `ProjectSessionForm.recipe`).

## Issues (by severity)

### Medium

- **Promise.all failure coupling** (already noted in `evaluation-report.md`): `ConnectionScreen` loads tools and agents together; one RPC failure clears both and shows a single error. This is a product/compatibility choice with a clean-code smell: unrelated concerns are fused in one async operation, which hurts resilience and makes behavior harder to reason about for operators on mixed-version stacks.

- **Cypress intercept duplication**: `interceptAllRpcs`, `interceptAllRpcsWithListSessionsFactory`, and `interceptAllRpcsWithListAgents` repeat the same auth/tools/agents/daemons/sessions/projects wiring. Any new required RPC or header change must be edited in multiple places—high drift risk.

### Low

- **Fixture drift**: `MOCK_DEFAULT_LIST_AGENTS` in `ConnectionScreen.cy.tsx` intentionally mirrors `dev.daemon.yaml` `allowed_agents` (comment acknowledges this). There is no compile-time or generated link; renaming or reordering on one side can silently desynchronize tests from local dev defaults.

- **Naming inconsistency**: UI strings and testids use “backend” (`backend-select`, `coalesceBackendAgentSelection`) while proto and daemon config use “agent”. Understandable historically, but it spreads cognitive load across the stack.

- **Redundant work in React**: `agents.map((a) => ({ id: a.id, label: a.label }))` appears in `defaultProjectSessionForm`, `ProjectSessionOptions`’s `useMemo`, and the `useEffect` that reconciles `projectForms`. `AgentInfo` is already shaped like `RpcAgent`; repeated mapping is minor noise.

- **Debug noise in libraries**: `agentOptions.ts` uses `console.debug` on every build/coalesce path; `agent_allowlist_rows` logs at `info` per allowlist row. For production bundles and default log levels, this is more instrumentation than stable “clean” code unless explicitly gated or removed in a hardening pass.

- **Stale phase comment**: The file header in `agentOptions.ts` (“Green phase: wire these into…”) reads like a TDD scratch note left in place; it should be updated to a durable description or removed.

- **`connection_service` list_tools vs agents**: `list_tools` still inlines label fallback logic analogous to agents (trim/empty → path). Not wrong, but if the pattern grows, a shared helper would avoid subtle rule divergence—today agents are centralized, tools are not.

## Refactor suggestions

1. **Cypress DRY (incremental)**: Extract a small internal helper (e.g. `setupConnectionRpcIntercepts({ sessions, listAgents, getSessions, daemonsOverride })`) that returns nothing but registers intercepts, so `interceptAllRpcs*` become thin wrappers. Keeps one place for `ListAgents` default body construction.

2. **Document or automate MOCK_DEFAULT_LIST_AGENTS**: Minimum: expand the existing comment with “when changing `dev.daemon.yaml` allowed_agents, update MOCK_DEFAULT_LIST_AGENTS.” Stronger: shared JSON fixture imported by Cypress (build step) or a single `fixtures/default-list-agents.json` checked against YAML in CI—only if the team accepts that tooling cost.

3. **Align vocabulary**: Prefer one term in new code (`agent` in helpers and props where it does not break public APIs) or document “backend (agent)” once in `ConnectionScreen` and keep `coalesceBackendAgentSelection` as a deprecated alias if rename is too wide.

4. **Split loads or scope errors**: If mixed-version daemons matter, fetch `listTools` and `listAgents` independently and merge state with granular errors (or degrade agents to empty with a specific message). Improves SRP of the effect and testability of each path.

5. **Logging**: Move per-row `log::info!` in `agent_allowlist_rows` to `debug` (or remove); strip or feature-flag `console.debug` in `agentOptions.ts` for production builds if the bundler does not already drop them.

6. **agentOptions header**: Replace “Green phase” with a one-line description of invariants (e.g. “options match ListAgents order; selection coalescing preserves valid prior id”).

7. **Optional**: Pass `AgentInfo[]` directly into `buildAgentSelectOptionsFromRpc` (widen type to `{ id: string; label: string }[]` only at the boundary) to delete repeated `.map` in `ConnectionScreen`.
