# Changeset: Screen sharing on the Hosts screen

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**PR**: (opened in wave 1; number recorded in wave 2)
**Type**: Feature
**Stack**: `#host-desktops` 4/8. Base: `feature/host-desktops/vnc-security`.

## Initial Discovery

[2026-10-18-host-desktops-hosts-mount-initial-discovery.md](./2026-10-18-host-desktops-hosts-mount-initial-discovery.md)

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| [2026-09-06-no-code-path-issues-gethosttooling.md](../todo/2026-09-06-no-code-path-issues-gethosttooling.md) | ✅ RESOLVED HERE | The Hosts screen mounts `HostRowTooling` fed by `GetHostTooling`. |
| [2026-09-06-tooling-probe-has-no-cache-or-dedup.md](../todo/2026-09-06-tooling-probe-has-no-cache-or-dedup.md) | ✅ RESOLVED HERE | Blocking in spirit ("resolve before it does, not after"): mounting the section turns the probe into a per-visit fan-out. Cache + in-flight dedup land in this node. |
| `packages/tddy-host-service/docs/code-issues/oversized-file-host-tooling.md` | ⚠ DURING | 775/500. The cache goes in a **new module** (`host_tooling_cache.rs`); `host_tooling.rs` grows only by the call site. Record the delta in the issue's measurement history. |

## Responsibility

- `HostsScreen` rendering, per **online** row, a tooling cell: a `GetHostTooling` call to that host,
  loading / failed states, and `HostRowTooling` with its readings (which brings `HostRowRemoteDesktop`
  and Connect).
- The daemon-side tooling result cache keyed by (daemon instance, github hostname) with a TTL, and
  in-flight dedup so concurrent callers share one probe run.

## Boundaries

- Does not change what the probe reports (node 7 adds channels), the Connect gating, or the overlay.
- Does not add a manual refresh control.
- Does not change `HostRowTooling`'s props beyond what mounting needs.

## Dependencies

| Parent | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n1–n3 | — | nothing (line predecessors only) | touch tddy-screen-sharing, tddy-rdp, tddy-vnc |

## Draft PR contract

```rust
// packages/tddy-host-service/src/host_tooling_cache.rs
pub struct ToolingCache<T: Clone> { .. }
impl<T: Clone + Send + 'static> ToolingCache<T> {
    pub fn new(ttl: Duration, clock: Arc<dyn Clock>) -> Self;
    pub async fn get_or_probe<F, Fut>(&self, key: ToolingKey, probe: F) -> T
        where F: FnOnce() -> Fut, Fut: Future<Output = T>;
}
```
```tsx
// packages/tddy-web/src/components/hosts/HostRowToolingCell.tsx
export function HostRowToolingCell(props: { instanceId: string; online: boolean }): JSX.Element;
```
Plus failing tests below.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — Cypress mounts `HostsScreen` with an in-memory backend; the cache is unit-tested with a fake clock and a counting probe.
**Concurrent with:** nodes 1 (after #626), 2, 3, 5
**Blocks:** nothing by behaviour. Node 7 edits `HostRowRemoteDesktop` and `host_tooling.rs` after it (file order only).

    #626 → n1      n1 → n6, n7, n8      n2 ⇢ n3 (signature only)      n2, n3, n6 → n8

## Affected Packages

- `tddy-web` — [README](../../../packages/tddy-web/README.md), [docs](../../../packages/tddy-web/docs/)
- `tddy-host-service` — [README](../../../packages/tddy-host-service/README.md), [docs](../../../packages/tddy-host-service/docs/)

## Related Feature Documentation

- PRD: `docs/ft/web/1-WIP/PRD-2026-10-18-host-desktops-hosts-mount.md`

## Summary

The host desktop feature becomes reachable: rows on `#/hosts` show the tooling section and Connect.

## Scope

- [ ] Tooling cell on online rows, via `useHostClient(HostService, hostId)`
- [ ] Cache + in-flight dedup in a new module
- [ ] Both TODOs closed; code issue measurement updated

## Technical Changes

### State A
- `HostsScreen.tsx:108` renders only `<HostRowTelemetry instanceId online />`.
- `HostRowTooling.tsx` (170) takes `{instanceId, git, githubCli, sshAgent?, remoteDesktop?}` and fetches nothing; mounted only by Cypress specs.
- Only production `GetHostTooling` caller: `ProjectsAppPage.tsx:179`.
- `host_tooling.rs` probes on every call; no cache (`2026-09-06-tooling-probe-has-no-cache-or-dedup.md`); `list_agent_models` cache is the shape to copy.

### State B
Online rows call `GetHostTooling` once per visit; the daemon answers repeated or concurrent calls
from one probe run within the TTL.

### Delta
| Package | Change |
|---|---|
| `tddy-web` | `HostsScreen.tsx`, `HostRowToolingCell.tsx` (new), page object additions |
| `tddy-host-service` | `host_tooling_cache.rs` (new), `host_tooling.rs` call site, `lib.rs` |

## Implementation Milestones
- [ ] M1 Cache + dedup with unit tests
- [ ] M2 Tooling cell mounted; Cypress acceptance green

## Testing Plan

Cypress component tests mount `HostsScreen` through `mountWithRpc` + `anInMemoryRpcBackend`, with a
host connection that carries media (`aHostConnection(...).reachedOverLiveKit()`), answering
`ListKnownHosts`-derived rows and `GetHostTooling`. Rust unit tests for the cache with an injected
clock and a probe that counts invocations and can be held open (to prove dedup).

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `offers connect on an online hosts row` | `packages/tddy-web/cypress/component/HostsScreenToolingMountAcceptance.cy.tsx` (new) | end-to-end reachability on the screen |
| `asks an online host for its tooling once per visit` | same | one call per row |
| `asks an offline host for nothing` | same | no call for offline rows |
| `says the tooling could not be read when the host does not answer` | same | failed state, not a blank cell |
| `two_concurrent_requests_for_one_host_run_one_probe` | `packages/tddy-host-service/src/host_tooling_cache.rs` (`tests`) | dedup |
| `a_request_within_the_ttl_runs_no_probe` | same | cache hit |
| `a_request_after_the_ttl_probes_again` | same | expiry |
| `different_hosts_are_cached_separately` | same | key isolation |

## Decisions & Trade-offs
- **Online rows only** — an offline host cannot answer, and asking would only produce failures.
- **Cache in the daemon, not the browser** — the TODO's concern is the host's subprocess fan-out, which only a daemon cache bounds.

## Technical Debt & Production Readiness

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

- [x] Record initial discovery (`2026-10-18-host-desktops-hosts-mount-initial-discovery.md`)
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
- [ ] Linting and formatting (scoped `cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-18-host-desktops-hosts-mount-initial-discovery.md`
- [ ] USER REVIEW — work complete
