# Changeset: telemetry-fanout

**Date:** 2026-09-06
**Status:** 🚧 In Progress
**Type:** New feature
**Stack:** `#hosts-screen` 2/8
**Branch:** `feature/hosts-screen/telemetry-fanout` → base `feature/hosts-screen/host-registry`
**PR:** [#454](https://github.com/uppin/tddy-coder/pull/454)

## Initial Discovery

[`./2026-09-06-telemetry-fanout-initial-discovery.md`](./2026-09-06-telemetry-fanout-initial-discovery.md)

## Related feature documentation

PRD: [`docs/ft/web/1-WIP/PRD-2026-09-06-telemetry-fanout.md`](../../ft/web/1-WIP/PRD-2026-09-06-telemetry-fanout.md)

Affected: [`host-stats-footer.md`](../../ft/web/host-stats-footer.md)

## Affected packages

| Package | Change |
|---|---|
| [`packages/tddy-web`](../../../packages/tddy-web) | per-host telemetry hook, Hosts row telemetry cell |

**No Rust package is touched by this node.**

## Responsibility

- Generalizing `useHostStats` to resolve its client per host via `useHostClient(ConnectionService, hostId)`.
- Opening exactly one `StreamHostStats` subscription per **online** host, only while the Hosts screen
  is mounted, and tearing them all down on unmount.
- The telemetry column on each Hosts row, reusing `CpuCoresIndicator`, `DiskSpaceIndicator` and
  `formatDiskFree`.
- Honest non-values: `—` for an offline host, a pending/unavailable state for a connecting or errored
  connection — never a fabricated zero.

## Boundaries

- Does **not** change `StreamHostStats`'s proto surface, its handler, or its cadence. No Rust change
  at all in this node.
- Does **not** add memory, load average or core count — `host-resources` owns those fields.
- Does **not** change `HostStatsFooter`'s behaviour or appearance; its zero-argument call must keep
  working against the selected host.
- Does **not** change the host registry, the `#/hosts` route, or the screen shell — those are node 1's.
- Does **not** introduce a generic streaming fan-out abstraction for the whole app. This node solves
  the Hosts screen's case; generalizing it is a later concern with more than one caller to learn from.

## Dependencies

What the parent PR delivers that this PR consumes. These surfaces are **its** to create; implementing
one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `host-registry` (#hosts-screen 1/8) | `ListKnownHosts` + `KnownHostEntry` (with `online`, `instance_id`) in `connection.proto`, regenerated in both languages; `HOSTS_ROUTE` / `isHostsPath`; `HostsAppPage` + `HostsScreen` and their prop types; the `shell-menu-hosts` nav entry | reads `online` and `instance_id` off each row to decide whether to subscribe and for which host; adds a column inside the existing `HostsScreen` row | add, remove or rename any `KnownHostEntry` field; change the registry or its persistence; change the route, the dispatch branch or the nav entry; alter the row's existing columns |

> ⚠ **Sequencing.** This node's tests drive `HostsScreen` rows, which exist only once node 1's wave-2
> commit is pushed. Rebase onto the parent before writing code (`/pr-stack-rebase`), and confirm
> `HostsScreen` is present at `HEAD` before starting `/green`.

## Draft PR contract

Lands first, so node 3 can branch off a real ref:

1. `useHostStats(hostId?)`'s generalized signature, and the per-host telemetry component's props —
   the surface node 3 adds memory/load/core-count fields to.
2. `HostRowTelemetry` and its props — the cell node 3 adds a memory reading to.
3. The failing tests below, red for this node's own missing implementation.

## Summary

Each host row on the Hosts screen gets live per-core CPU and free disk, streaming. The daemon already
produces exactly this data; today only the *selected* host's feed is ever consumed. This node fans the
existing feed out across the fleet — the first streaming fan-out in the codebase, since `useHostFanOut`
is unary-shaped and cannot express a subscription.

## Scope

- [x] `useHostStats` generalized to a per-host client
- [x] Subscription policy: online hosts only, screen-scoped lifetime
- [x] Telemetry column on the Hosts row, reusing the existing indicators
- [x] Honest states for offline / connecting / errored
- [x] Cypress component acceptance tests

## Technical changes

### State A

- `useHostStats()` takes no arguments and resolves `useDaemonClient(ConnectionService)` — the
  **selected** daemon only (`packages/tddy-web/src/rpc/useHostStats.ts:38`).
- Its single consumer is `HostStatsFooter` (`components/sessions/HostStatsFooter.tsx:29`), the bottom
  strip of the sessions drawer.
- `useHostFanOut` (`rpc/useHostFanOut.ts:103`) exists for fleet-wide reads but is **unary-shaped**
  (`read(...): Promise<readonly T[]>`); it cannot carry a subscription.
- `useHostClient(service, hostId)` (`rpc/connections/registry.tsx:207`) already resolves a client per
  host, returning `null` when no provider can reach that host.
- The Cypress backend exposes a single global `hostStatsStreamCount()`
  (`cypress/support/rpc/connectionServiceBackend.ts:637`), not a per-host tally.

### State B

- `useHostStats` accepts an optional host id; with none it behaves exactly as today.
- The Hosts screen subscribes per online row, tearing down on unmount.
- Offline rows render `—` and never subscribe; connecting/errored rows render a pending or unavailable
  state.

### Delta

**`packages/tddy-web`**
- `src/rpc/useHostStats.ts` — per-host client resolution; the no-argument call preserved.
- `src/components/hosts/HostRowTelemetry.tsx` — the row's telemetry cell and its three states.
- `src/components/hosts/HostsScreen.tsx` — the Telemetry column: one `<th>`, one `<td>` rendering
  `HostRowTelemetry`, and the import. Nothing else in that file is this node's. It had to wait for
  node 1 to render rows at all; until then the cell stood alone and the acceptance spec mounted it
  directly.
- `cypress/support/pages/hostsScreenPage.ts` — `rows()` now matches `tr[data-testid^="hosts-row-"]`
  instead of excluding known child suffixes. See the note below.
- `cypress/support/rpc/connectionServiceBackend.ts` — **not touched**; the existing global
  `hostStatsStreamCount()` proves every count under test.
- `cypress/support/pages/hostsScreenPage.ts` — telemetry cell selectors (node 1 creates the file).

## Implementation milestones

- [x] `useHostStats(hostId)` resolving per host, footer still green
- [~] Per-host stream tally in the backend helper — **dropped, by the testing plan above.** The
      properties under test are counts, not attributions; the existing global `hostStatsStreamCount()`
      proves them, and the discriminator a per-host tally needs would have cost a proto change.
- [x] Telemetry cell rendering live values for an online host — asserts `42.1 GB` and the exact
      per-core percentages.
- [x] Offline / connecting / errored states, no fabricated values
- [x] Teardown on unmount verified — by `hostStatsSubscription.test.ts`, which asserts the iterator
      is closed, including one that never emitted. The backend counter could not have shown this.
- [x] `./dev bun run cypress:component` green for the touched specs

## Testing plan

**Level: Cypress component only.** This node adds no Rust code, so there is nothing to unit-test in
Rust. The behaviour under test — how many subscriptions are opened, for which hosts, and what a row
renders when there is no feed — is entirely a client contract, and `mountWithRpc` exercises it against
a real ConnectRPC client over a fake server.

**Options considered.**

| Option | Verdict |
|---|---|
| Cypress component with `mountWithRpc` + `anInMemoryRpcBackend` | **Chosen.** Real client, real streaming semantics via async generators, deterministic |
| `cy.intercept` wire-level fakes (`cypress/support/rpc/protoRpc.ts`) | Rejected — cannot observe LiveKit-transport RPC at all |
| A React unit test of the hook in isolation | Rejected — the property that matters ("one stream per online host") is only observable where the registry resolves clients |

**The obstacle this plan had to clear — resolved, with no proto change.** `StreamHostStatsRequest`
carries only `session_token` (`connection.proto`), and `mountWithRpc` hands **every** host the same
in-memory transport (`cypress/support/rpc/inMemory.tsx:39`), so a fake cannot tell two hosts'
subscriptions apart.

It does not need to. The properties under test are **counts**, not attributions: two online hosts must
open two streams, and an online+offline pair exactly one. The backend's existing global
`hostStatsStreamCount()` (`connectionServiceBackend.ts:637`) proves both. **No per-host tally is
added and no proto change is made** — the discriminator that would have been needed for attribution
buys nothing the acceptance criteria actually ask for.

**Coverage.** Every AC in the PRD maps to a named test below.

## Acceptance tests

**`packages/tddy-web/cypress/component/HostsScreenTelemetryAcceptance.cy.tsx`**

| Test | Validates |
|---|---|
| `shows_live_cpu_and_disk_for_an_online_host` | AC-1 |
| `updates_the_cpu_bars_as_fresh_readings_stream_in` | AC-2 |
| `opens_exactly_one_stats_subscription_per_online_host` | AC-3 |
| `opens_no_subscription_for_an_offline_host_and_shows_a_dash` | AC-4 |
| `shows_an_unavailable_state_rather_than_zeroes_when_a_host_is_unreachable` | AC-5 |
| `tears_down_every_subscription_when_the_screen_unmounts` | AC-6 |

**`packages/tddy-web/cypress/component/HostStatsFooterAcceptance.cy.tsx`** (existing spec, must stay green)

| Test | Validates |
|---|---|
| `sources_both_indicators_from_a_single_StreamHostStats_subscription` | AC-7 — no regression |

## Decisions & trade-offs

- **Generalize `useHostStats` rather than write a second hook.** One streaming contract, one place
  where the AbortError-on-unmount rule lives. The footer's call site is unchanged.
- **Subscribe only for online hosts.** An offline host has no connection to subscribe through
  (`connectHost` returns `null`), so the alternative is not "try anyway" but "render honestly".
- **Screen-scoped lifetime, not app-scoped.** N streams at 5 s each is real load; nothing should pay
  it while looking at another screen.
- **No generic streaming fan-out abstraction yet.** `useHostFanOut` was built for unary reads and
  `useModelRegistryFanOut` had to be written separately; a third caller is the right time to
  generalize, not the first.
- **Reachability is the host directory's answer, not a null client.** The plan assumed an unreachable
  host resolves a `null` client. It does not: `LiveKitConnectionProvider.connectHost`
  (`rpc/connections/liveKit.tsx:257`) returns a connection for **any** non-empty host id once a room
  exists — by design, since "who is in the roster" is the host *directory*'s business, not the
  provider's. So the cell subscribes only for a host `useDaemons()` names, which is the same rule
  `useHostFanOut` already applies to peers. A row the directory does not name renders `unavailable`.
- **A pending state distinct from unavailable.** Between opening the stream and its first frame there
  is no reading — not a reading of zero — so the cell shows `…` rather than idle bars.
- **No fabricated values.** CLAUDE.md forbids fallbacks. A zeroed CPU bar for an unreachable host is a
  lie an operator will act on.

## Technical debt & production readiness

_(populated during development)_

### Red phase — closing the validation gaps

Driven by the `/pr-wrap` findings above. Two of the three gaps turned out to be **unobservable
through the component fake**, which changed where their tests belong.

**Proved, not assumed.** Two throwaway probes against `createRouterTransport` showed it propagates
neither an abort nor a consumer's `break` to the server handler: a `finally` in the fake's
`streamHostStats` never runs, so a live-stream gauge can never fall back to zero. A backend counter
therefore *cannot* observe teardown, and the gauge written for it was removed rather than shipped as
an assertion surface that cannot fail.

A per-subscription CPU discriminator was dropped for the same kind of reason: a cell reading the
**selected daemon** for every row still opens one subscription per cell, so it consumes the
discriminator exactly as a correct implementation does. It distinguishes nothing.

Both properties moved to `bun test` unit suites, following the house split `useHasCapability` uses —
a pure function that is unit-tested, and a thin hook that wraps it.

| Suite | Pins | Status |
|---|---|---|
| `src/rpc/hostStatsSubscription.test.ts` (6 tests) | AC-6 — a subscription is **closed**, not merely ignored, including one that never emitted | 🔴 `subscribeHostStats` does not exist |
| `src/components/hosts/hostTelemetryState.test.ts` (4 tests) | AC-3/4/5 — which host a row reads, and that an unreadable row never answers `undefined` (the selector) | 🔴 `telemetryFeedFor` does not exist |
| `cypress/component/HostsScreenTelemetryAcceptance.cy.tsx` (9 tests) | AC-1..AC-5 | 🟢 8 · 🔴 1 (`-telemetry-offline` marker) |
| `cypress/component/HostStatsFooterAcceptance.cy.tsx` (6 tests) | AC-7 — no regression | 🟢 6 |

**What the rewritten acceptance spec fixed:**

- AC-1 was `.should("exist")` twice against a zero-byte disk fixture, so "shows free disk" was
  satisfied by rendering "0 B free". It now asserts `42.1 GB` and the exact per-core percentages.
- `not.contain.text("0%")` could not fail — nothing renders that string (`CpuCoresIndicator` emits
  `style="height: 0%"`, not text). Replaced with "neither metric is rendered".
- The pending state had no test and no selector; a `hostStatsSilent` scenario now drives it.
- `OFFLINE_HOST` named a host passed `online: true` in two tests. Hosts are now named by identity
  (`HOST_A` / `HOST_B`) and each test states the state it means.
- Raw selectors, `data-` attributes and glyphs left the test bodies for named page-object accessors
  (`expectCpuCores`, `expectFreeDisk`, `expectNoReading`); every test reads Given/When/Then.
- An offline row was asserted through the shared cell's `—`, which `DiskSpaceIndicator` also renders
  for a null reading — so it could not tell "offline" from "live row with no disk figure". It now
  requires an offline marker of its own.

**Backend helper change** — additive only (+8 lines): a `hostStatsSilent` knob. No per-host tally, no
proto change; the existing global `hostStatsStreamCount()` still carries the count assertions.

### Green phase — the extracted seams

| Suite | Result |
|---|---|
| `src/rpc/hostStatsSubscription.test.ts` | 🟢 6/6 |
| `src/components/hosts/hostTelemetryState.test.ts` | 🟢 4/4 |
| `cypress/component/HostsScreenTelemetryAcceptance.cy.tsx` | 🟢 9/9 |
| `cypress/component/HostStatsFooterAcceptance.cy.tsx` | 🟢 6/6 — no regression |
| `bun run --filter tddy-web test:unit` | 🟢 1113/1113 |

- `subscribeHostStats` iterates the stream **manually** rather than with `for await`, so
  `unsubscribe()` can call `iterator.return()` directly. A `for await` cannot: parked awaiting a
  frame that may never arrive, it can never reach a `break` — which is why a silent host was never
  let go of. The flag is re-checked on the far side of the await, so a frame already in flight when
  the caller let go is dropped rather than delivered to a component that stopped listening.
- `telemetryFeedFor` is pure and returns `string | null`, never `undefined`. That spelling means
  "follow the daemon selector", and it is the one answer that would put one machine's CPU under
  another machine's name.
- `useHostStats` keeps its signature and tri-state exactly; only its effect body moved.

**One extra change, outside the plan.** `packages/tddy-web/package.json`'s `test:unit` enumerates
directories, and `src/components/hosts` was not among them — `hostTelemetryState.test.ts` is the
first test file to live there, so it would have passed locally and never run in CI. Added the
directory (one word). A test that does not run is worse than no test, since it reads as coverage.

### Wiring the column onto the row

Node 1 (#453) greened and pushed its row rendering, which unblocked the one part of
`## Responsibility` that had been deferred. After rebasing onto its new tip the column went in: a
`Telemetry` header and a cell per row, placed after the `Status` / `Last seen` pair rather than
between them, since those two are node 1's paired liveness columns.

Two screen-level tests now cover it — an online host's reading appearing **on its own row**, and an
offline row saying it has none — driving the real `HostsAppPage` rather than a bare cell. They were
red before the column existed.

**One parent-owned file needed a fix.** `hostsScreenPage.rows()` selected
`[data-testid^="hosts-row-"]` minus an enumerated list of known child suffixes
(`-liveness`, `-last-seen`). Adding any new per-row cell therefore made that cell count as a row, and
node 1's "sorts online hosts above offline ones" began seeing six rows instead of two. Since
`## Dependencies` explicitly sanctions "adds a column inside the existing `HostsScreen` row", the
selector had to tolerate one, so it now matches by element — `tr[data-testid^="hosts-row-"]` — which
needs no exception list and cannot regress the same way for node 3's memory reading. Node 1's five
tests pass unchanged. **Flagged for node 1's author**, since it is their file.

### Second validation pass — teardown did not actually work

The first implementation released the stream with `iterator.return()`. Against the real transport
that is a **no-op**, and the suite was green only because the fakes offered a `return()` production
never has. `@connectrpc/connect`'s `handleStreamResponse` wraps every server-stream in

```js
// Create a new iterable to omit throw/return.
return { [Symbol.asyncIterator]: () => ({ next: () => it.next() }) };
```

Every client here goes through `createClient`, so `stream.return` is always `undefined`; the optional
call type-checked and did nothing. Nothing was cancelled, the daemon kept emitting to unmounted rows,
and the parked `next()` never settled — so the loop retained the stream and the caller's handler for
the life of the page. AC-6 was ticked on a fake that could not fail.

**Fixed** by cancelling the call, which is how every other stream hook in this repo does it
(`useTaskListStream`, `useHostFanOut`, `useLiveKitRooms`, …): `open` now takes an `AbortSignal`,
`unsubscribe()` aborts, and `useHostStats` passes it to `streamHostStats`. Aborting rejects the parked
`next()`, so the loop unwinds and releases. `iterator.return()` is still called for iterators that do
expose one. A new test uses a **`next`-only** fake — the shape the real client actually returns — so
this class of miss cannot pass again.

**Also fixed in this pass:**

- A pre-outage reading was re-shown as live. `perCorePercent`/`disk` survived a `client` change, so a
  host that went away and came back rendered its old CPU and disk until the new feed's first frame —
  and indefinitely if it never reported. The subscribing effect now clears both first. Rows are keyed
  by `instanceId`, so there was never cross-host bleed; this was within one row.
- The teardown log lost its guard in the extraction, so an ordinary unmount would report a dropped
  feed. Restored, matching `useSessionNotifications` and `useWorktreeStatsStream`.
- Two untested exit paths — a feed that ends cleanly, and a transport that cannot open at all.
- The CPU-only-pending branch had no accessor and no test; `hostCpuPerCore: []` reaches it.
- `telemetryFeedFor`'s "never `undefined`" test was subsumed (`toBeNull()` already rejects
  `undefined`) and could not fail on its own. Removed — the `string | null` return type enforces it.
- Naming: the iterator was called `reading`, which everywhere else in this node means a CPU/disk
  sample.

Mutation-checked: a `for await` + break implementation fails three of these tests, dropping the
post-await recheck fails exactly one, and dropping `close()` from the loop's `finally` fails exactly
one — each test kills a distinct plausible mistake.

## Refactoring needed

### From /red

- [x] Extract the stream loop out of `useHostStats` into `subscribeHostStats` so teardown is
      testable; the hook becomes the thin wrapper.
- [x] Extract the feed decision into `telemetryFeedFor`, so `HostRowTelemetry` states which host it
      reads rather than deriving it inline.
- [x] Give the offline branch its own `-telemetry-offline` marker.
- [x] `useHostStats` unsubscribe is cooperative (`cancelled` + `break`), so a host that never reports
      is never let go of. `subscribeHostStats` must close the iterator outright.

## Validation results

### Red phase (draft-PR contract)

- `HostsScreenTelemetryAcceptance.cy.tsx` — **6 tests, 6 red**, every one on this node's own missing
  cell (`hosts-row-…-cpu` / `…-telemetry-unavailable` never found).
- The spec mounts `HostRowTelemetry` directly rather than `HostsScreen`: row rendering belongs to
  `#hosts-screen 1/8` and is not implemented, so driving the screen would make every failure
  attributable to *that* node instead of this one.
- No Rust change in this node, so no Rust verification applies.

### Green phase

- `HostsScreenTelemetryAcceptance.cy.tsx` — **6/6 green**.
- `HostStatsFooterAcceptance.cy.tsx` — **6/6 green**; `useHostStats` was left unmodified in this
  phase, so the footer's zero-argument call is unchanged in behaviour.
- One production file changed: `src/components/hosts/HostRowTelemetry.tsx`. No test, page object,
  test-support, proto or `src/gen/` file was touched, and no Rust change applies.
- Not run: the rest of the Cypress suite (~50 min, 207 specs) — left to CI.

### /pr-wrap validation

**Fixed in this pass** (all re-verified, 12/12 specs green):

- `useHostStats.ts` — the module doc claimed a host with no reachable connection resolves to a `null`
  client. It does not: a registered common room answers for **any** host id, so a non-null client is
  not evidence of reachability. This is the false premise that misled the green phase.
- `useHostStats.ts` — the `@param` said `null` follows the daemon selector; it subscribes to nothing.
  Collapsing the tri-state (`hostId ?? undefined`) would report the selected daemon's CPU under an
  unreachable host's name, so the doc is now explicit about why the two are not the same.
- `HostRowTelemetry.tsx` — a reading carrying disk but no CPU rendered `CpuCoresIndicator` with an
  empty array: a blank bar strip that reads as *all cores idle*, contradicting this file's own
  no-fabrication docblock. The CPU slot is now decided per metric and stays pending instead.
- `HostRowTelemetry.tsx` — the state table under-counted (three listed, four rendered) and ran in the
  opposite order to the branches it documents.

**Open — blocks readiness, needs a decision:**

1. **`## Responsibility` is not delivered in full.** "The telemetry column on each Hosts row" is not
   in the branch: `HostRowTelemetry` is imported only by its own spec and is mounted nowhere in
   `src/`. Node 1 (#453) is still at its draft-contract commit, so `HostsScreen` renders no rows to
   put a column on. This PR's `## Dependencies` explicitly allows "adds a column inside the existing
   `HostsScreen` row", so the wiring is this node's job, deferred by sequencing.
2. **AC-6 is not pinned.** `hostStatsStreamCount()` counts opens and is never decremented
   (`connectionServiceBackend.ts:337,570,637`), so `tears_down_every_subscription_when_the_screen_unmounts`
   passes whether teardown works or not. It also cannot work against this backend: the generator parks
   on `await new Promise<never>(...)`, so `cancelled = true` never breaks the `for await`. Needs a live
   gauge (increment on open, decrement in the generator's `finally`).
3. **Per-host attribution is not pinned** — the PR's central claim. Every host shares one transport
   and the only evidence is a global count, so an implementation reading the **selected daemon** for
   every row (`useHostStats(online && inDirectory ? undefined : null)`) passes all six tests. Fixable
   without a proto change: yield a different `perCorePercent` per subscription and assert the two rows
   differ.
4. **AC-1 is weakly pinned.** Both assertions are `.should("exist")`; no test sets `hostDisk`, so the
   backend defaults to `0n` and the cell renders "0 B free" — blessed as "shows free disk". The
   sibling `HostStatsFooterAcceptance.cy.tsx` does this properly with a real figure.
5. **AC-5 is half-pinned.** `not.contain.text("0%")` is tautological — nothing renders that string
   (`CpuCoresIndicator` emits `style="height: 0%"`, not text). The pending state has no test and no
   page-object selector, and no backend knob makes the stream error.

## TODO

- [x] Record initial discovery (`2026-09-06-telemetry-fanout-initial-discovery.md`)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
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

- [`2026-09-06-host-resources.md`](./2026-09-06-host-resources.md) — branch
  `feature/hosts-screen/host-resources`
