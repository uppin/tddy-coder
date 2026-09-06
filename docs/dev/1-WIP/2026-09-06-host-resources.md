# Changeset: host-resources

**Date:** 2026-09-06
**Status:** 🚧 In Progress
**Type:** Modification to an existing feature
**Stack:** `#hosts-screen` 3/8
**Branch:** `feature/hosts-screen/host-resources` → base `feature/hosts-screen/telemetry-fanout`
**PR:** [#455](https://github.com/uppin/tddy-coder/pull/455)

## Initial Discovery

[`./2026-09-06-host-resources-initial-discovery.md`](./2026-09-06-host-resources-initial-discovery.md)

## Related feature documentation

PRD: [`docs/ft/web/1-WIP/PRD-2026-09-06-host-resources.md`](../../ft/web/1-WIP/PRD-2026-09-06-host-resources.md)

Affected: [`host-stats-footer.md`](../../ft/web/host-stats-footer.md)

## Affected packages

| Package | Change |
|---|---|
| [`packages/tddy-service`](../../../packages/tddy-service) | `HostStatsEvent` gains memory + load blocks and a core count |
| [`packages/tddy-daemon`](../../../packages/tddy-daemon) | `HostStats` trait methods, `SysinfoHostStats` impl, handler wiring, test doubles |
| [`packages/tddy-web`](../../../packages/tddy-web) | memory indicator + formatter, Hosts row and footer rendering |

## Responsibility

- New readings on the `HostStats` trait for memory and load average, and the explicit core count.
- Their `SysinfoHostStats` implementation, including the platform cases where a load average does not
  exist.
- The `HostStatsEvent` memory and load blocks, carried on the **fast** tick.
- Updating both in-test `HostStats` doubles so the cadence tests keep seeing every reading.
- Rendering memory on the Hosts row and the new readings in `HostStatsFooter`, with `—` where a
  reading is genuinely unavailable.

## Boundaries

- Does **not** change the cadence model. Two timers stay two timers; no third interval, no third
  builder parameter.
- Does **not** change how disk is sampled or which filesystem is reported — still the project dir's
  mount, still the slow tick.
- Does **not** add per-mount disk reporting.
- Does **not** touch the fan-out mechanics, the subscription policy or the per-host tally — node 2's.
- Does **not** touch the registry, `ListKnownHosts`, the route or the nav entry — node 1's.
- Does **not** probe any software installed on a host — nodes 4–8.

## Prerequisites

Open items in [`docs/dev/TODO.md`](../TODO.md) this PR runs into.

### ⚠ DURING — `connection_service.rs` is 19,600 lines

`docs/dev/TODO.md` § *`connection_service.rs` is 19,600 lines* (source:
subagent-conversation-inference, 2026-08-29), flagged rather than acted on.

This node adds to that file. Across the `#hosts-screen` stack, **nodes 1, 3, 4 and 6** modify it —
nodes 2, 5, 7 and 8 do not — so the stack makes a known problem measurably worse in four places.

The TODO is explicit that a split "needs to be its own PR", because `ConnectionServiceImpl`'s ~60
private fields would have to become `pub(crate)` and several hundred in-file tests would repoint. So
this is **recorded, not fixed here**.

Worth leaving for whoever does that split: the host registry, tooling probe and prompt handlers this
stack adds form a coherent host-facing group, which is not among the seams the TODO currently lists.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `telemetry-fanout` (#hosts-screen 2/8) | `useHostStats(hostId?)` resolving a client per host; `HostRowTelemetry` and its props; the per-host `hostStatsStreamCount(hostId)` seam in the Cypress backend | adds memory to the existing telemetry cell and reads the new event fields through the same hook | change the subscription policy, the per-host client resolution, the teardown behaviour, or the stream tally |
| `host-registry` (#hosts-screen 1/8) | the Hosts row and screen | renders one more reading inside the existing row | change the row's identity columns or the registry |

> ⚠ **Sequencing.** The new fields reach a row only through node 2's hook. Rebase onto the parent
> (`/pr-stack-rebase`) and confirm `useHostStats(hostId)` exists at `HEAD` before `/green`.

## Draft PR contract

Lands first, so node 4 can branch off a real ref:

1. The extended `HostStats` trait signature and the new `HostStatsEvent` blocks, regenerated in both
   languages — every later node compiles against this proto file and must not race it.
2. Updated test doubles, so the workspace still builds.
3. The failing tests below.

## Summary

Finish the telemetry surface. `sysinfo` is already a dependency and already long-lived for correct
per-core deltas, but is never asked for memory or load; core count exists only as
`per_core_percent.len()`. An operator can currently see that a host's CPU is busy but not that it is
out of memory — the more common reason a session fails.

## Scope

- [x] `HostStats` trait: memory, load average, core count
- [x] `SysinfoHostStats` implementation, including the no-load-average platforms
- [x] Proto blocks + both regenerations
- [x] Handler wiring on the fast tick
- [x] Both test doubles updated and participating in cadence sequencing
- [x] Memory indicator + byte formatter reuse
- [x] Hosts row and footer rendering, with `—` for unavailable readings
- [⚠] Rust unit/integration tests + Cypress component tests — all green, but the **Hosts row**
      memory cell (`hosts-row-<id>-memory`) is rendered and unasserted; see Validation results

## Technical changes

### State A

- `trait HostStats` has exactly two methods (`host_stats.rs:57-62`): `cpu_per_core_percent` and
  `disk_for_project_dir`.
- `SysinfoHostStats` (`:69-137`) calls `refresh_cpu_usage()` and `sysinfo::Disks::new_with_refreshed_list()`.
  It never calls `refresh_memory()` and never reads a load average.
- `HostStatsEvent` (`connection.proto:2250-2264`) has `cpu` and `disk`, both documented as always
  populated.
- `stream_host_stats` (`connection_service.rs:16529-16595`) runs `cpu_tick` (5 s) and `disk_tick`
  (60 s), each pushing a full event.
- Two in-test implementors exist: `FakeHostStats` and `SequencedHostStats`.
- Web renders `CpuCoresIndicator` and `DiskSpaceIndicator`; `hostStatsFormat.ts` has `formatDiskFree`
  (`:14`) and `clampCorePercent` (`:23`). No memory anywhere.

### State B

- The trait reports memory, load average (optional) and core count.
- `HostStatsEvent` carries a memory block and a load block, refreshed on the fast tick; disk unchanged
  on the slow tick.
- A host that cannot report a load average says so on the wire; the UI renders `—`.
- The Hosts row shows memory; the footer shows the new readings alongside its existing ones.

### Delta

**`packages/tddy-service`**
- `connection.proto`: `HostMemoryStats` (total / available bytes), `HostLoadStats` (1/5/15 minute
  averages plus an explicit "reported" discriminator), a core-count field, and their `HostStatsEvent`
  fields. Field numbers must be genuinely free.

**`packages/tddy-daemon`**
- `src/host_stats.rs`: trait methods, the return types, `SysinfoHostStats` implementation.
- `src/connection_service.rs`: populate the new blocks on the fast tick; update `FakeHostStats` and
  `SequencedHostStats`.

**`packages/tddy-web`**
- `src/components/sessions/MemoryIndicator.tsx` — beside the disk indicator, same shape.
- `src/components/sessions/hostStatsFormat.ts` — a memory formatter following
  `formatDiskFree(availableBytes: number | bigint)`; no second byte formatter.
- `src/rpc/useHostStats.ts` — surface the new readings on the hook's result type.
- `src/components/hosts/HostRowTelemetry.tsx` — memory on the row.
- `src/components/sessions/HostStatsFooter.tsx` — the new readings in the footer.

## Implementation milestones

- [x] Trait + return types; workspace builds with both doubles updated
- [x] `SysinfoHostStats` memory + load + core count
- [x] Proto + both regenerations
- [x] Fast-tick population; cadence tests still meaningful
- [x] Memory indicator and formatter
- [x] Row + footer rendering, `—` for unavailable
- [x] `./test -p tddy-daemon` and the touched Cypress specs green

## Testing plan

**Levels.**

| Level | Where | Why |
|---|---|---|
| Unit (Rust) | `packages/tddy-daemon/src/host_stats.rs` `#[cfg(test)]` | The "load average not reported" case is provider behaviour and must be provable without a stream |
| Integration (Rust) | `packages/tddy-daemon/src/connection_service.rs` `#[cfg(test)]` | Which tick carries which block is only observable at the handler |
| Component (Cypress) | `packages/tddy-web/cypress/component/` | `—` versus `0.00` is a rendering contract, and it is the one an operator is misled by |

**Options considered.** Asserting load average against the real machine was rejected outright — it is
environment-dependent and would make the suite flaky, and CLAUDE.md forbids test-only branches in
production code. The provider is injected (`with_host_stats`, `connection_service.rs:2016`), so a
double supplies both the reporting and the non-reporting case deterministically.

Cadence is asserted with short injected intervals via `with_host_stats_intervals`
(`connection_service.rs:2032`) plus the bounded `next_event` timeout helper, so a hang fails rather
than passing.

**Coverage.** Every AC maps to a named test.

## Acceptance tests

**`packages/tddy-daemon/src/host_stats.rs`** (unit)

| Test | Validates |
|---|---|
| `reports_total_and_available_memory_for_the_host` | AC-1 |
| `reports_the_logical_core_count` | AC-2 |
| `reports_no_load_average_on_a_platform_that_does_not_provide_one` | AC-4 — the honesty case |

**`packages/tddy-daemon/src/connection_service.rs`** (integration)

| Test | Validates |
|---|---|
| `stream_host_stats_emits_memory_and_load_immediately_on_subscribe` | AC-1, AC-3 |
| `stream_host_stats_refreshes_memory_on_the_fast_cadence` | AC-5 |
| `stream_host_stats_still_refreshes_disk_on_the_slow_cadence` | AC-6 — no regression |
| `stream_host_stats_marks_load_average_unreported_when_the_provider_has_none` | AC-4 |

**`packages/tddy-web/cypress/component/HostsScreenTelemetryAcceptance.cy.tsx`** (extends node 2's spec)

| Test | Validates |
|---|---|
| `shows_memory_for_an_online_host` | AC-7 |
| `renders_a_dash_rather_than_zero_when_a_host_reports_no_load_average` | AC-4 |

**`packages/tddy-web/cypress/component/HostStatsFooterAcceptance.cy.tsx`**

| Test | Validates |
|---|---|
| `shows_memory_beside_the_existing_disk_and_cpu_indicators` | AC-8 |

## Decisions & trade-offs

- **No third timer.** Memory and load change on CPU's timescale; a separate interval would add a timer
  and a builder parameter for no user-visible gain.
- **Core count on every event.** It is effectively static, and one `u32` per event is cheaper than a
  separate RPC or a special first frame.
- **"Not reported" is on the wire, not inferred client-side.** Inferring it from a zero would make a
  genuinely idle machine indistinguishable from an unsupported platform.
- **Reuse the byte formatter.** A second, near-identical formatter is how two of them drift apart.
- **Breaking the trait is accepted.** All implementors are in-repo; the compiler finds every one.

## Technical debt & production readiness

_(populated during development)_

## Refactoring needed

_(populated by each validation phase)_

## Validation results

### Red phase (draft-PR contract)

- `cargo build -p tddy-service` — pass; `bun run --filter tddy-web generate` — pass.
- **The trait break landed exactly as predicted.** Adding three methods to `HostStats` failed the
  build at every implementor, including two `FakeHostStats` construction sites in the test module.
  That is the reason the plan accepted a breaking trait change rather than defaulted methods: the
  compiler enumerates the sites, so no double can silently keep reporting a stale shape.
- `SequencedHostStats` gained its own `memory_reads` counter, so
  `refreshes_memory_on_the_fast_cadence` proves a *fresh* read rather than a repeated snapshot. Had
  memory reused the CPU counter the assertion would have passed while demonstrating nothing.
- Added `still_refreshes_disk_on_the_slow_cadence` as a regression guard: moving memory onto the fast
  tick is precisely the change that could drag the expensive disk walk along with it.

### Green phase

Implementation landed; the three `TODO(host-resources)` placeholders are consumed and no
`unimplemented!()` remains in this PR's files.

- `cargo fmt --all --check`, `cargo clippy -p tddy-daemon --all-targets -- -D warnings`,
  `cargo build` (workspace) — all pass.
- `./test -p tddy-daemon` — every suite passes except
  `cursor_cli_session_acceptance::cursor_cli_sandbox_start_succeeds_when_sandbox_backend_available`,
  which panics on `ConnectionServiceImpl::self_arc called before set_self_handle`
  (`connection_service.rs:1871`). Pre-existing harness-wiring failure unrelated to this node: this
  PR's diff touches lines 103, ~16626 and ~18033 and never mentions `self_arc`/`set_self_handle`.
- Cypress `HostResourcesAcceptance` (3), `HostsScreenTelemetryAcceptance` (12) and
  `HostStatsFooterAcceptance` (6) — 21 passing, 0 failing.

**The load-average honesty case is a compile-time platform split**, not a runtime fallback:
`sysinfo` 0.33.1 compiles a real implementation only for `macos/ios/linux/android/freebsd`
(`sysinfo-0.33.1/src/lib.rs:36-39`) and returns an all-zero `LoadAvg` elsewhere, so the unsupported
targets return `None` and the zero sentinel never reaches the wire.

**`formatDiskFree` now delegates to `formatBytesFree`** rather than keeping a second byte-formatting
body, which is the "reuse the byte formatter" decision made concrete. Its signature and output are
unchanged.

**Base moved twice during green.** Node 2 (`telemetry-fanout`) rewrote its branch and then advanced
it three commits. The first required `git rebase --onto <new base> <recorded old tip>`; a plain
rebase would have replayed node 2's superseded commits into this PR's diff. Three conflicts, all
resolved on the ownership rule: `connectionServiceBackend.ts` by **union** (node 2's
`hostStatsSilent` + this node's memory/load scenario fields), and `useHostStats.ts` /
`HostRowTelemetry.tsx` by **letting the base win on mechanics** — node 2's `subscribeHostStats(...)`
teardown and `telemetryFeedFor({...})` resolution were kept, with only this node's readings added
inside them.

#### Open gaps (carried into `/validate-changes`)

1. **⚠ The Hosts row memory cell is rendered but unasserted.** `HostRowTelemetry` renders
   `hosts-row-<id>-memory`, which `## Responsibility` calls for, but no test covers it. This was
   *not* blocked by node 1: node 2 established `mountCells`, which mounts `HostRowTelemetry`
   directly and bypasses the unimplemented screen, so the test is writable now. The acceptance-test
   table above names row tests in `HostsScreenTelemetryAcceptance.cy.tsx` that the red phase
   consolidated into `HostResourcesAcceptance.cy.tsx` as footer-only coverage.
2. **ℹ `logicalCores` is surfaced on `UseHostStatsResult` but consumed nowhere.** The proto carries
   it and the red phase pinned it on the hook's result type; `CpuCoresIndicator` still derives its
   bars from `perCorePercent`. No speculative rendering was added because no test asks for one.

## TODO

- [x] Record initial discovery (`2026-09-06-host-resources-initial-discovery.md`)
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

- [`2026-09-06-host-identity.md`](./2026-09-06-host-identity.md) — branch
  `feature/hosts-screen/host-identity`
