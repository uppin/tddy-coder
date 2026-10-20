# Changeset: Screen sharing on the Hosts screen

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**PR**: [#656](https://github.com/uppin/tddy-coder/pull/656)
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

- [x] Tooling cell on online rows, via `useHostClient(HostService, hostId)`
- [x] Cache + in-flight dedup in a new module
- [x] Both TODOs closed; code issue measurement updated

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
- [x] M1 Cache + dedup with unit tests
- [x] M2 Tooling cell mounted; Cypress acceptance green

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
| `different_os_users_on_one_host_are_cached_separately` | same | key isolation by OS user (added in red, see below) |

**Status (contract written 2026-10-18):** all 4 Cypress tests fail on behaviour (the tooling never
appears on a row — `HostRowToolingCell` is published but not yet mounted by `HostsScreen`); all 5 cache
tests fail on `todo!("TODO(hosts-mount): implement ToolingCache::new")`.

**Deviations from the draft contract, recorded at red:**
- **`ToolingKey` carries the OS user** — `{daemon_instance_id, os_user, github_hostname: Option<String>}`,
  not just (daemon instance, github hostname). The probe is per OS user (`git config --global` and
  `gh auth status` read that user's `$HOME`, `host_tooling.rs` module doc), so a key without it would
  serve one operator's git identity / `gh` login to another — the same reason `list_agent_models`
  keys its cache by account. Pinned by the fifth unit test.
- **`Clock` is a trait defined in `host_tooling_cache.rs`** (`trait Clock: Send + Sync { fn now(&self) -> Instant }`,
  plus `SystemClock`). The only workspace clocks are `type Clock = Arc<dyn Fn() -> Instant + Send + Sync>`
  aliases in `tddy-credentials` and `tddy-accounts`, neither a dependency of `tddy-host-service`; the
  contract's `Arc<dyn Clock>` asked for a trait.
- **Page object:** `hostToolingCellPage` (`loading`, `failed`, `expectCouldNotBeRead`) added to
  `cypress/support/pages/hostsScreenPage.ts`; the cell's test ids are `hosts-row-<id>-tooling-loading`
  and `hosts-row-<id>-tooling-failed`, and the failed text must match `/could not (be )?read/i`.

## Decisions & Trade-offs
- **Online rows only** — an offline host cannot answer, and asking would only produce failures.
- **Cache in the daemon, not the browser** — the TODO's concern is the host's subprocess fan-out, which only a daemon cache bounds.

## Technical Debt & Production Readiness

## Refactoring Needed
### From @red (TDD Red Phase)
- `aKnownHost` is now defined in four Hosts specs (including `HostsScreen.cy.tsx` and this node's spec) — a shared `knownHosts.ts` builder in `cypress/support/rpc/` would remove the copies.
- Green (decided): every answer is cached for the full TTL, **including a failed reading**. The probe reports failures as readings (`ProbeOutcome`), not as errors, so there is no error answer to exclude, and re-running it per request would reinstate the fan-out. The one error path, a probe that panicked on the blocking pool, is kept as its message for the same TTL (30 s, `HOST_TOOLING_CACHE_TTL` in `service.rs`); `get_or_probe` returns an infallible `T`, so excluding it would need an API change nothing asks for.
- Green (done): wiring `ToolingCache` into `get_host_tooling` (`service.rs`, not `host_tooling.rs` — the probe call is in `service.rs:486-495`) needs a TTL constant and a `HostServiceImpl` field; no test pins the call site.
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
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (approved with the wave-1 batch, 2026-10-18)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests for touched packages; full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (scoped `cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-18-host-desktops-hosts-mount-initial-discovery.md`
- [ ] USER REVIEW — work complete
