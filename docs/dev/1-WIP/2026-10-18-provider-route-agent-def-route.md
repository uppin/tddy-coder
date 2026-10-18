# Changeset: Harness profiles — agent definitions that name a provider account

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**Type**: Feature
**Stack**: #provider-route 8/9. Base: `feature/provider-route/opencode-acp`.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-18-provider-route-agent-def-route-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| [2026-10-08-drain-tddy-discovery-into-purpose-named-crates-and-remove-it.md](../todo/2026-10-08-drain-tddy-discovery-into-purpose-named-crates-and-remove-it.md) | ⚠ DURING | `agent_def.rs` is the drain's first planned move. Add the profile block in a sibling module so the eventual move carries it unchanged; do not start the drain here. |

## Responsibility

The **agent-definition** level of route precedence. Owns `SpecializedAgentDef.harness: Option<HarnessProfile>`
(`HarnessProfile { harness, provider_account, model }`), its load-time validation (no `base_url` /
`api_key` beside it), `ListAgents` rows for profiles, and the start-time resolution that maps a profile
to its session type / agent and feeds `RouteSources.agent_definition`.

## Boundaries

- Does not change OpenAI-loop definitions, registry assistants or `replaces:`.
- Does not change `choose_route` (node 2) or the user-default feed (node 5).
- No UI editor (YAML only).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/provider-route/claude-route` (2/9) | `RouteSources`, `choose_route`, claude-cli routed start | feeds `agent_definition`; profile with `harness: claude-cli` starts through it | change precedence |
| `feature/provider-route/tool-route` (4/9) | tool-session route delivery | profiles naming tool-session agents start through it | change the route file |
| `feature/provider-route/opencode-cli` (3/9), `muse-cli` (6/9), `opencode-acp` (7/9) | **Not consumed.** Harness ids come from node 2's `Harness` enum, which declares every harness; no edge from these nodes | — | — |

## Draft PR contract

```rust
// tddy-discovery (agent_def sibling module)
#[derive(Deserialize)] pub struct HarnessProfile { pub harness: String, pub provider_account: String, pub model: String }
// SpecializedAgentDef
#[serde(default)] pub harness: Option<HarnessProfile>,
pub fn validate_profile(def: &SpecializedAgentDef) -> Result<(), String>;
// tddy-agent-launch
pub(crate) fn profile_start(def: &SpecializedAgentDef) -> Option<(SessionKind, RouteChoice)>;
```

## Green wave

**Wave:** 4 of 5
**Greenable independently:** no — needs node 4's tool-session route behaviour and node 2's claude-cli start.
**Concurrent with:** `muse-cli` (6/9), `opencode-acp` (7/9)
**Blocks:** none

    n1 → n2      n2 → n3, n4, n5      n3 → n6, n7, n9      n4 → n7, n8      n6 → n9

## Affected Packages

`tddy-discovery` (def), `tddy-session-agents` / `tddy-daemon-kernel` (agent list rows), `tddy-agent-launch` (start).

## Related Feature Documentation

- PRD: [PRD-2026-10-18-provider-route-agent-def-route.md](../../ft/daemon/1-WIP/PRD-2026-10-18-provider-route-agent-def-route.md)

## Scope

- [ ] Profile block + validation
- [ ] Agent-list rows ("<label>" from the def)
- [ ] Start: profile → session type / agent + `agent_definition` source; account resolved by label or id in the starter's vault

## Testing Plan

Def parse/validation unit tests; launch acceptance for a claude-cli profile and a codex tool-session profile.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_claude_cli_profile_starts_claude_cli_on_its_account_and_model` | `packages/tddy-agent-launch/tests/harness_profile_acceptance.rs` (new) | start |
| `a_session_level_account_overrides_the_profiles` | same | precedence |
| `a_profile_naming_an_account_the_person_does_not_hold_is_refused` | same | refusal |
| `a_profile_with_a_base_url_is_rejected_at_load` | `packages/tddy-discovery/tests/harness_profile_def.rs` (new) | validation |
| `profiles_appear_in_the_agent_list` | `packages/tddy-session-agents/tests/harness_profile_rows.rs` (new) | rows |

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **Profiles are agent definitions** (house preference for agent-driven config over flags).

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-18-provider-route-agent-def-route-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests for touched packages; full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (scoped)
- [ ] Wrap documentation (/wrap-context-docs)
- [ ] USER REVIEW — work complete
