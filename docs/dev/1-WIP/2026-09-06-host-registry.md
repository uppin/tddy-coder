# Changeset: host-registry

**Date:** 2026-09-06
**Status:** 🚧 In Progress
**Type:** New feature
**Stack:** `#hosts-screen` 1/8 (root node)
**Branch:** `feature/hosts-screen/host-registry` → base `master`

## Initial Discovery

[`./2026-09-06-host-registry-initial-discovery.md`](./2026-09-06-host-registry-initial-discovery.md)

## Related feature documentation

PRD: [`docs/ft/web/1-WIP/PRD-2026-09-06-host-registry.md`](../../ft/web/1-WIP/PRD-2026-09-06-host-registry.md)

Affected: [`app-shell.md`](../../ft/web/app-shell.md), [`url-state-routing.md`](../../ft/web/url-state-routing.md),
[`projects-screen-multi-host.md`](../../ft/web/projects-screen-multi-host.md)

## Affected packages

| Package | Change |
|---|---|
| [`packages/tddy-service`](../../../packages/tddy-service) | `connection.proto`: `ListKnownHosts` + its messages |
| [`packages/tddy-daemon`](../../../packages/tddy-daemon) | new `host_registry` module, discovery hook, RPC handler |
| [`packages/tddy-web`](../../../packages/tddy-web) | `#/hosts` route, `HostsAppPage`/`HostsScreen`, nav entry |

## Responsibility

- A **persisted** daemon-side registry of every host seen, keyed by `daemon_instance_id`, retaining
  `label`, `repos_base_path`, `max_attachment_bytes`, `first_seen_unix_ms`, `last_seen_unix_ms`.
- Recording into it from the existing peer-discovery path, including the local daemon's own entry.
- The unary `ConnectionService.ListKnownHosts` RPC, with `online` computed per call against the live
  eligible-daemon source.
- The `#/hosts` screen: route, dispatch branch, nav entry, `HostsAppPage` + `HostsScreen`, one row per
  known host showing online/offline, last-seen, label, instance id and repos base path.

## Boundaries

- Does **not** change the host **directory** (`useHostDirectory`, the LiveKit and serving sources) or
  the `DaemonSelector`. How a host is named, selected and reached is untouched.
- Does **not** open a `HostConnection` per row. This screen makes exactly one RPC.
- Does **not** touch `StreamHostStats` or add any telemetry to a row — `telemetry-fanout` owns that.
- Does **not** add memory / load / core-count fields — `host-resources` owns those.
- Does **not** probe anything on a host (git, `gh`, ssh-agent, VNC, RDP) — nodes 4–8 own those.
- Does **not** widen `ConnectionCapability`. That type means what the *wire* carries; the host-facts
  sense introduced by later nodes is named separately.
- Does **not** delete a host, ever — there is no removal RPC in this node.

## Dependencies

This is the **root node** of the `#hosts-screen` stack. It has no in-stack parent, and its base is
`master`.

It was planned on top of `feature/optional-livekit/lazy-session-room` while the `#optional-livekit`
stack was still open. **That stack has since landed** — all nine PRs (#437–#451) were squash-merged to
`master` on 2026-09-06 — so this node was rebased onto `master` directly and the repoint that was
pending at planning time is done. The host-connection model, the host directory and the capability
gating this stack builds on are all on `master`.

## Draft PR contract

Lands first, so node 2 can branch off a real ref:

1. `ListKnownHosts` + `KnownHostEntry` in `connection.proto`, regenerated in both languages — the
   signature node 2's row rendering compiles against.
2. `HOSTS_ROUTE` / `isHostsPath`, and `HostsAppPage` / `HostsScreen` exporting their prop types —
   the components node 2 adds a telemetry column to.
3. The failing tests below, red for this node's own missing implementation.

## Summary

Make host existence durable, and give it a screen. Today a host that leaves the LiveKit room vanishes
from tddy entirely; there is no record it ever existed. This node records every host the daemon sees,
persists that record, and renders it at `#/hosts` with an online/offline state and a last-seen time.

## Scope

- [ ] `host_registry` module in `tddy-daemon`, with persistence
- [ ] Recording hook in the peer-discovery path
- [ ] `ListKnownHosts` proto + regeneration (Rust + TypeScript)
- [ ] `ListKnownHosts` handler
- [ ] `#/hosts` route, dispatch branch, nav entry
- [ ] `HostsAppPage` + `HostsScreen` with row rendering and sort
- [ ] Rust unit + integration tests
- [ ] Cypress component acceptance tests

## Technical changes

### State A

- Host existence is **live-only and ephemeral**. `CommonRoomPeerRegistry`
  (`livekit_peer_discovery.rs:267-269`) is an `RwLock<HashMap<String, EligibleDaemonInfo>>` replaced
  wholesale by `sync_from_room` (`:277`). Nothing is written to disk.
- The web mirrors that: `mergeHostDirectory` (`useHostDirectory.tsx:25`) rebuilds from the live
  roster; `resolveSelectedDaemonInstanceId` (`selectedHost.ts:38-51`) returns only ids still present.
- The only host-related persistence anywhere is the *selected* host id in `sessionStorage`
  (`SELECTED_DAEMON_STORAGE_KEY`, `selectedHost.ts:14`) — per-tab, and not a host list.
- `ListEligibleDaemons` (`connection.proto:71`) returns `EligibleDaemonEntry { instance_id, label,
  is_local }` for **currently connected** daemons only.
- There is no `/hosts` route and no `HostsScreen`.

### State B

- A `HostRegistry` in `tddy-daemon` owns the durable record. Reads tolerate a missing or corrupt
  file by starting empty; a failed **write** surfaces as an error rather than being swallowed
  (the `FileGitHubTokenStore` posture, `github_token_store.rs:38-73`).
- Peer discovery records appearances and stamps `last_seen_unix_ms` on disappearance. Entries are
  never removed.
- `ListKnownHosts` returns every registry entry, with `online` computed at call time by intersecting
  with `EligibleDaemonSource::list_eligible_daemons()`.
- `#/hosts` renders one row per entry, online-first then by label.

### Delta

**`packages/tddy-service`**
- `connection.proto`: `rpc ListKnownHosts(ListKnownHostsRequest) returns (ListKnownHostsResponse);`
  plus `ListKnownHostsRequest`, `KnownHostEntry`, `ListKnownHostsResponse`.
- Field numbers must be genuinely free — never a reused retired number.

**`packages/tddy-daemon`**
- New `src/host_registry.rs`: the entry type, the store trait, a file-backed implementation, and the
  `online` intersection helper.
- `src/livekit_peer_discovery.rs`: record into the registry from `sync_from_room`.
- `src/connection_service.rs`: the handler, plus an injected `Arc<dyn HostRegistry>` field and a
  builder override for tests (mirroring `with_host_stats`, `:2016`).
- `src/connection_tonic_adapter.rs`: the unary adapter entry.

**`packages/tddy-web`**
- `src/routing/appRoutes.ts`: `HOSTS_ROUTE`, `isHostsPath`.
- `src/components/hosts/HostsAppPage.tsx`, `HostsScreen.tsx`, `hostRowFormat.ts` (last-seen formatting).
- `src/index.tsx`: one import + one dispatch branch.
- `src/components/shell/DaemonNavMenu.tsx`: `shell-menu-hosts`.
- `src/gen/connection_pb.ts`: regenerated.

## Implementation milestones

- [ ] `HostRegistry` + file persistence, unit-tested standalone
- [ ] Discovery hook recording appearances and last-seen
- [ ] Proto + both regenerations, workspace builds
- [ ] `ListKnownHosts` handler with auth and the `online` intersection
- [ ] Route + nav entry reachable
- [ ] `HostsScreen` rows, sort, and last-seen formatting
- [ ] All tests green; `cargo clippy -- -D warnings` clean

## Testing plan

**Levels.** Three, deliberately:

| Level | Where | Why this level |
|---|---|---|
| Unit (Rust) | `packages/tddy-daemon/src/host_registry.rs` `#[cfg(test)]` | Persistence, last-seen stamping and never-delete are pure store behaviour; testing them through the RPC would obscure which layer failed |
| Integration (Rust) | `packages/tddy-daemon/src/connection_service.rs` `#[cfg(test)]` | The `online` intersection is a *join* of registry and live source — it only exists at the handler |
| Component (Cypress) | `packages/tddy-web/cypress/component/` | Row rendering, offline presentation and sort are UI contracts; `mountWithRpc` gives a real client against a fake server |

**Options considered.** A single end-to-end test through a live daemon was rejected: restart
durability (AC-4) would need a real process restart, which `./test` does not provide and which the CI
gate deliberately does not cover. A temp-dir-backed store re-opened in-process proves the same
property deterministically.

**Coverage required.** Every AC in the PRD maps to at least one named test below.

## Acceptance tests

**`packages/tddy-daemon/src/host_registry.rs`** (unit, `#[cfg(test)]`)

| Test | Validates |
|---|---|
| `a_host_recorded_once_is_still_listed_after_the_store_is_reopened` | AC-4 — durability across restart |
| `reopening_the_store_preserves_the_original_first_seen_timestamp` | AC-4 — `first_seen` is not re-stamped |
| `a_host_that_goes_away_keeps_its_entry_and_updates_last_seen` | AC-5 — never delete |
| `a_corrupt_registry_file_reads_as_an_empty_registry` | AC-9 — tolerate corruption |
| `a_registry_that_cannot_be_written_reports_the_failure` | AC-9 — do not swallow a write failure |

**`packages/tddy-daemon/src/connection_service.rs`** (integration, `#[cfg(test)]`)

| Test | Validates |
|---|---|
| `list_known_hosts_rejects_an_invalid_token` | AC-7 |
| `list_known_hosts_marks_a_host_in_the_live_roster_as_online` | AC-2 |
| `list_known_hosts_marks_a_recorded_host_absent_from_the_roster_as_offline` | AC-3 |
| `list_known_hosts_always_includes_the_local_daemon` | AC-6 |

**`packages/tddy-web/cypress/component/HostsScreenAcceptance.cy.tsx`**

| Test | Validates |
|---|---|
| `lists_a_connected_host_as_online` | AC-2 |
| `lists_a_previously_seen_host_as_offline_with_its_last_seen_time` | AC-3 — the node's whole point |
| `sorts_online_hosts_above_offline_ones` | AC-8 |
| `reaches_the_hosts_screen_from_the_navigation_menu` | AC-1 |

Page object: `packages/tddy-web/cypress/support/pages/hostsScreenPage.ts`.
Mount: `mountWithRpc(withSelectedDaemon(<HostsAppPage />), backend)`.

## Decisions & trade-offs

- **`online` is computed, never stored.** A persisted online flag is wrong the moment a daemon exits
  without notice. Computing it per call costs one cheap intersection.
- **Never delete a host.** An operator looking at an unreachable host is precisely the case this
  screen exists for. Deletion, if ever wanted, is a separate explicit action in a later change.
- **One unary RPC, not a stream.** Registry membership changes rarely; a stream here would add the
  `tx.closed()` teardown obligation (`packages/tddy-codegen/docs/server-streaming.md`) for no gain.
  Node 2 introduces streaming where it is actually warranted.
- **Persistence in the daemon, not the browser.** Explicit product decision: the list must survive a
  reload and be shared across browsers.

## Technical debt & production readiness

_(populated during development)_

## Refactoring needed

_(populated by each validation phase)_

## Validation results

_(populated by each validation command)_

## TODO

- [x] Record initial discovery (`2026-09-06-host-registry-initial-discovery.md`)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run all tests (`./test`) — verify 100% pass
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
- [ ] Linting and formatting (`cargo clippy -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs)
- [ ] USER REVIEW — work complete, decide next steps

## Successor PRs

- [`2026-09-06-telemetry-fanout.md`](./2026-09-06-telemetry-fanout.md) — branch
  `feature/hosts-screen/telemetry-fanout`
