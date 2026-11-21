# Changeset: An orchestrator reaches each child by its best route and releases a terminal it claimed

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — route choice per child, turn_input declared for two more agents, a new lease release RPC through three layers, lease cleanup on session end
**Stack**: `#orchestra` 7/9 — branch `feature/orchestra/orchestrator-delivery`, base `feature/orchestra/access-elicitation`

## Initial Discovery

[2026-11-21-orchestrator-delivery-initial-discovery.md](./2026-11-21-orchestrator-delivery-initial-discovery.md)

## Affected Packages

- **tddy-daemon-rpc**: `ListProjectSessions` reports each child's `delivery_route`; `ReleaseSessionTerminal`; `HomeTerminalLeases::forget`
- **tddy-coding-agent-specs**: `turn_input: terminal_paste` for Cursor and Muse
- **tddy-terminal-rpc**: `ReleaseTerminalControl` RPC
- **tddy-cli-sessions**: `control_lease` release; leases dropped when the session ends
- **tddy-tool-engine**: `ReleaseSessionTerminal` tool

## Related Feature Documentation

- PRD: [PRD-2026-11-21-orchestrator-delivery.md](../../ft/daemon/1-WIP/PRD-2026-11-21-orchestrator-delivery.md)

## Summary

For each child, the daemon tells the orchestrator how to reach it: `native` (a Claude child on the same host, by the peer name n2 records), `inbox` (`PostSessionMessage` + wake, for local agents with tddy-tools), or `terminal` (`SendSessionMessage` typed into its TUI — preferred for Coder/SSH children). Cursor and Muse declare `turn_input: terminal_paste`, so `SendSessionMessage` reaches them too. An orchestrator can release a terminal it claimed (`ReleaseSessionTerminal`), and a claim is forgotten when its session ends.

## Responsibility

- Route choice and its listing; terminal preference for targets.
- Cursor/Muse `turn_input`.
- Lease release end to end and cleanup on end.

## Boundaries

- Does not name or launch peers (n2).
- No ACP route.
- No prompt text (n9).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n2 child-back-channel` | `SessionRecord.native_peer_name`, `BackChannel` | lists the peer name and picks `native` from it | pin or change peer names, or render guidance |

## Draft PR contract

The first push of this PR, before its implementation: `DeliveryRoute`, `ReleaseTerminalControl` proto + `ControlLease::release`, `HomeTerminalLeases::forget`, `ReleaseSessionTerminal` definition; failing tests below.

## Green wave

**Wave:** 2 of 4
**Greenable independently:** partly — release and Cursor/Muse turn input are greenable now; the `native` route test needs n2's recorded peer name
**Concurrent with:** n6
**Blocks:** n9

Real dependency edges (the line is n1 · n2 · n3 · n4 · n5 · n6 · n7 · n8 · n9):

    n1 path-reach → n6 access-elicitation, n8 orchestrator-lsp
    n6 access-elicitation → n8 orchestrator-lsp
    n2 child-back-channel → n7 orchestrator-delivery
    n1 … n8 → n9 orchestrator-prompt

Waves: 1 = n1, n2, n3, n4, n5 · 2 = n6, n7 · 3 = n8 · 4 = n9 (run `1 1 1 1 1 2 2 3 4`).
Inside wave 1, blockers lead: n1 (3 transitive dependents: n6, n8, n9), n2 (2: n7, n9), then n3, n4, n5
(1 each: n9) in foundational order. Inside wave 2: n6 (2: n8, n9) before n7 (1: n9). n3–n5 and n7 sit
in the line only because `gh stack` needs a line; none depends on its neighbour.

## Prerequisites

### Related — [2026-11-13-acp-turn-input-is-not-delivered.md](../todo/2026-11-13-acp-turn-input-is-not-delivered.md)

Unchanged; ACP agents keep their refusal.

### Related — [2026-11-14-muse-answer-session-question.md](../todo/2026-11-14-muse-answer-session-question.md)

Terminal paste for Muse makes messaging work; answering a Muse question stays out of scope.

### ⚠ DURING — [oversized-file-bridge.md](../../../packages/tddy-terminal-rpc/docs/code-issues/oversized-file-bridge.md)

The release RPC is served in `service.rs`; `bridge.rs` is not touched.

## Scope

- [ ] Delivery route per child, terminal preferred for Coder/SSH
- [ ] `turn_input` for Cursor and Muse
- [ ] Terminal claim release + lease cleanup on session end
- [ ] Feature doc `project-concept.md` § Sending a session a turn and § Project-home tools updated through the wrap

## Technical Changes

### State A (Current)

`turn.rs:104-128` reads `turn_input` (Claude only); `ClaimSessionTerminal` steals, no release; `home_terminal_leases.rs` remember/token_of only.

### State B (Target)

Every child has a route; Cursor/Muse take terminal turns; claims can be released and die with their session.

## Acceptance Tests

`packages/tddy-daemon-rpc/tests/orchestrator_delivery_acceptance.rs`:
- [ ] `a_local_claude_child_is_listed_with_the_native_route_and_its_peer_name`
- [ ] `a_coder_child_is_listed_with_the_terminal_route_and_a_message_is_typed_into_its_tui`
- [ ] `a_message_reaches_a_cursor_child_by_terminal_paste`
- [ ] `a_message_reaches_a_muse_child_by_terminal_paste`
- [ ] `a_released_terminal_can_be_claimed_by_the_web_screen_without_stealing`
- [ ] `releasing_a_claim_the_caller_does_not_hold_is_refused`
- [ ] `a_sessions_terminal_leases_are_forgotten_when_it_ends`

## Decisions & Trade-offs

- Both orchestrator roles (`project-home` main and `orchestrator`) get this change: they share
  `is_orchestrator_role` and one wiring (developer, 2026-11-21).
- The stack sits on `#target-parity` 7/7 (#975), because that stack rewrites the remote hook path
  and answer files this stack builds on (developer, 2026-11-21).
- For Coder/SSH children the TUI is the preferred route (developer requirement 9).
- The orchestrator does not need to request releasing its own claim — release is unconditional for its own lease (developer requirement 10).

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-21-orchestrator-delivery-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
