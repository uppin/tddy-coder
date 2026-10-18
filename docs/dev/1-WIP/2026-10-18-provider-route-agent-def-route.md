# Changeset: Harness profiles — agent definitions that name a provider account

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**Type**: Feature
**Stack**: #provider-route 8/9. Base: `feature/provider-route/opencode-acp`. PR: [#669](https://github.com/uppin/tddy-coder/pull/669).

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-18-provider-route-agent-def-route-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| [2026-10-08-drain-tddy-discovery-into-purpose-named-crates-and-remove-it.md](../todo/2026-10-08-drain-tddy-discovery-into-purpose-named-crates-and-remove-it.md) | ⚠ DURING | `agent_def.rs` is the drain's first planned move. Add the profile block in a sibling module so the eventual move carries it unchanged; do not start the drain here. |

## Responsibility

The **agent-definition** level of route precedence. Owns `tddy_discovery::harness_profile::{HarnessProfile, load_harness_profiles, parse_harness_profile, harness_profile_rows}` — a **separate type**, not a field on `SpecializedAgentDef` — the change to `load_agent_defs` that leaves profile files to their own loader, the agent-list rows for profiles, and `tddy_agent_launch::harness_profile_start::{ProfileStart, profile_start}` with the start-time feed of `RouteSources.agent_definition`.

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
// tddy-discovery/src/harness_profile.rs
#[serde(deny_unknown_fields)]   // so base_url / api_key beside a profile are refused
pub struct HarnessProfile { pub name: String, pub label: Option<String>, pub harness: String, pub provider_account: String, pub model: String }
pub fn load_harness_profiles(dir: &Path) -> Vec<HarnessProfile>;
pub fn parse_harness_profile(yaml: &str) -> Result<HarnessProfile, String>;
pub fn harness_profile_rows(profiles: &[HarnessProfile]) -> Vec<(String, String)>;
// tddy-agent-launch/src/harness_profile_start.rs
pub struct ProfileStart { pub session_type: Option<String>, pub agent: Option<String>, pub choice: RouteChoice, pub model: String }
pub fn profile_start(profile: &HarnessProfile) -> Result<ProfileStart, String>;
```

An account the starting person does not hold is refused by node 2's `resolve_session_route` (`NOT_FOUND`); `/green` resolves a label to an id before calling it.

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
| `a_profile_parses_from_its_yaml` / `a_profile_with_a_base_url_is_rejected_at_load` / `profiles_and_openai_definitions_load_side_by_side` / `profiles_appear_in_the_agent_list` | `packages/tddy-discovery/tests/harness_profile_def.rs` (new) | profile type, loaders side by side, rows |
| `a_claude_cli_profile_starts_claude_cli_on_its_account_and_model` / `a_codex_profile_starts_a_tool_session_with_the_codex_agent` / `a_profile_naming_no_known_harness_is_refused` / `a_session_level_account_overrides_the_profiles` | `packages/tddy-agent-launch/src/harness_profile_start_tests.rs` (new) | start mapping, precedence |
| `a_codex_profile_hands_its_tool_session_the_profiles_account` | same (`through_the_tool_session_route`) | a profile's tool session runs through #665's route file on the profile's account — the real edge n4 → n8 |

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **Profiles are their own type** (wave 2): `SpecializedAgentDef.base_url` is required and the def has 73 struct literals; a profile is a harness pointed at an account, not an endpoint.
- **Profiles are agent definitions** (house preference for agent-driven config over flags).

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Red-phase record (wave 2, 2026-10-18)

Scoped: `tddy-discovery`, `tddy-agent-launch` — clippy `-D warnings` clean.

- `harness_profile_def` 4/4 and `harness_profile_start_tests` 4/4 fail at this node's `todo!()`.
- Added at the wave-order check (third commit): `a_codex_profile_hands_its_tool_session_the_profiles_account`, the planned codex tool-session profile test, which the contract commit had left out. It fails first at node 1's `preset()`, and exercises #663's `choose_route` and #665's `tool_session_route_json` — without it this node's tests needed only node 2, which would have put it in wave 3 behind two wave-4 nodes.

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
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests (per-node gate waived by the developer on 2026-10-18 — "finish remaining waves without stopping"; review at stack hand-off)
- [x] TDD Red — write failing unit/integration tests
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
