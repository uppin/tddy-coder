# 2026-11-04 — `provider_route::Harness` still enumerates the agents a route body is chosen for

**Category:** Deferred removal
**Source:** `#agent-tools` 12/12 (#801), `TODO(agent-registry)` in
`packages/tddy-agent-backend/src/provider_route.rs` and `packages/tddy-agent-launch/src/session_provider_route.rs`

## What is there

The registry now says, per agent, which providers a route can point it at and how
(`coding_agent.route.providers` / `shape`), and `ListAgents` reports them. The route *bodies*
(`claude_code_route`, `codex_route`, `opencode_route`, `muse_route`) are still selected by the
`Harness` enum, whose `IDS` table names 11 session types and agents, and `Harness::route_support`
matrix (`route_supported`, `harness_routable`) is not read from the spec. Eight test suites in
`tddy-agent-backend/tests` and about 25 files in `tddy-agent-launch` name `Harness::…` variants.

An agent dropped into `<tddyhome>/tools` has no `Harness`: it runs on its own setup, and a start
that names a provider account for it is refused (`no provider route serves this agent`).

## Why

Replacing the enum is a rewrite of the route layer and of every test that names a variant. It does
not change what the registry enumerates for the picker or the gates, and it widens #801 past the
registry. The changeset names `provider_route::Harness` under Removed.

## What to decide

Select the route body by `RouteSupport` (`shape` + `providers`), key refusals by agent id, delete
`Harness`, and pin that the spec's `route.providers` are what the body accepts (a drift test between
the two exists nowhere today).

**Changeset ACs left open:** "`RouteSupport` replaces `provider_route::Harness`'s matrix".
