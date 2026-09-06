# Changeset: host-identity

**Date:** 2026-09-06
**Status:** 🚧 In Progress
**Type:** New feature
**Stack:** `#hosts-screen` 4/8
**Branch:** `feature/hosts-screen/host-identity` → base `feature/hosts-screen/host-resources`

## Initial Discovery

[`./2026-09-06-host-identity-initial-discovery.md`](./2026-09-06-host-identity-initial-discovery.md)

## Related feature documentation

PRD: [`docs/ft/web/1-WIP/PRD-2026-09-06-host-identity.md`](../../ft/web/1-WIP/PRD-2026-09-06-host-identity.md)

Affected: [`projects-screen-multi-host.md`](../../ft/web/projects-screen-multi-host.md)

## Affected packages

| Package | Change |
|---|---|
| [`packages/tddy-service`](../../../packages/tddy-service) | the host tooling RPC + its messages |
| [`packages/tddy-daemon`](../../../packages/tddy-daemon) | probe module, handler, peer routing hookup |
| [`packages/tddy-web`](../../../packages/tddy-web) | tooling section on the Hosts row |

## Responsibility

- The **host tooling probe RPC** on `ConnectionService` — the shape nodes 5 and 7 extend — carrying a
  `daemon_instance_id` so the existing peer routing relays it.
- A daemon-side probe module reading the host's git identity and `gh` status via
  `spawner::run_capture_as_user`, as the host's OS user, with a bounded timeout.
- A response type that distinguishes every outcome: git configured / not configured / probe failed,
  and `gh` absent / logged out / authenticated as a login.
- The tooling section on each Hosts row, labelling the `gh` login as the **host's**.

## Boundaries

- Does **not** introduce a general remote-execution RPC. Two fixed probes only; a "run this on host X"
  primitive is a far larger security surface and is not this node's to create.
- Does **not** touch `ExecuteTool` / `StreamExecuteTool` or relax their `session_id` requirement.
- Does **not** touch `packages/tddy-github`, tddy's OAuth login, `GitHubTokenStore`, or the
  `GITHUB_TOKEN` / `GH_TOKEN` REST path. Unrelated identities.
- Does **not** *change* anything on a host. This node reads only; no `git config --set`, no `gh auth login`.
- Does **not** probe the ssh-agent (node 5) or VNC/RDP (node 7), though it owns the RPC shape they extend.
- Does **not** reuse `ConnectionCapability`, which means what the wire carries.
- Does **not** modify telemetry, the registry, the route or the nav entry.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `host-registry` (#hosts-screen 1/8) | `ListKnownHosts` / `KnownHostEntry` with `instance_id` and `online`; `HostsScreen` rows | addresses the probe at each row's `instance_id`, and only for online hosts | change the registry, its persistence, or the row's identity columns |
| `host-resources` (#hosts-screen 3/8) | the extended `HostStatsEvent` blocks in `connection.proto` | shares the same proto file — must not race its field numbering | edit or renumber any telemetry message |
| `telemetry-fanout` (#hosts-screen 2/8) | the per-host subscription policy | follows the same "online hosts only" policy for probes | change the subscription policy or the stream tally |

> ⚠ **Shared-file hazard.** Nodes 3, 4, 5 and 7 all add messages to `connection.proto`. Pick genuinely
> free field numbers and never reuse a retired one; a conflict here resolves by **renumbering**, never
> by taking one side.

## Draft PR contract

Lands first, so node 5 can branch off a real ref:

1. The tooling-probe RPC and its request/response messages, regenerated in both languages — the shape
   node 5 adds ssh-agent facts to and node 7 adds desktop reachability to.
2. The probe module's trait/seam, so a double can be injected the way `with_host_stats` allows.
3. The failing tests below.

## Summary

Report each host's configured git identity and the state of `gh` on it. Neither is available anywhere
today: no production code reads `git config user.name`/`user.email`, and nothing shells out to `gh`.
The operator consequence is that when a host commits under the wrong identity, or its `gh` is logged
out, tddy says nothing.

## Scope

- [ ] Probe RPC + messages, both regenerations
- [ ] Probe module with an injectable seam and a bounded timeout
- [ ] git identity probe (configured / not configured / failed)
- [ ] `gh` probe (absent / logged out / authenticated as)
- [ ] Handler with auth and peer routing
- [ ] Tooling section on the Hosts row
- [ ] Rust unit/integration tests + Cypress component tests
- [ ] **Verify behaviour under the supervisor's tool/env allowlist**

## Technical changes

### State A

- **git identity:** no production reader. Every `user.name` / `user.email` hit is a test fixture
  writing into a temp repo. No git-config abstraction exists; `DaemonConfig::git`
  (`config.rs:191-201`) holds only `ssh_command`.
- **`gh`:** absent from product code entirely. `grep` for `gh auth` / `Command::new("gh")` over
  `packages/**` returns zero. `packages/tddy-github` is an OAuth login crate, not a `gh` wrapper.
- **Running a command as the host user:** `spawner::run_capture_as_user`
  (`spawner.rs:736-830`) exists and handles `HOME` / `PATH` / privilege drop. Unix only; `:829-834`
  is a non-Unix stub that bails.
- **Session-less host exec:** does not exist. `ExecuteToolRequest` requires `session_id`
  (`connection.proto:1545-1551`).
- **Peer routing:** `rpc_served_by_peer` (`connection_service.rs:9171-9183`) already relays unary RPCs.

### State B

- One unary tooling-probe RPC, relayed to the addressed host.
- A probe module reading both facts as the host's OS user, bounded by a timeout, reporting each
  distinct outcome explicitly.
- Each Hosts row shows the git identity and the `gh` state.

### Delta

**`packages/tddy-service`**
- `connection.proto`: the probe RPC, its request (`session_token`, `daemon_instance_id`), and a
  response carrying a git-identity block and a `gh` block, each with an explicit state discriminator.

**`packages/tddy-daemon`**
- `src/host_tooling.rs`: the probe trait, its real implementation over `run_capture_as_user`, the
  timeout, and the outcome types.
- `src/connection_service.rs`: handler, auth, peer routing, an injected `Arc<dyn HostTooling>` field
  and a builder override for tests (mirroring `with_host_stats`, `:2016`).
- `src/connection_tonic_adapter.rs`: the unary adapter entry.

**`packages/tddy-web`**
- `src/components/hosts/HostRowTooling.tsx` — the row section and its states.
- `src/components/hosts/HostsScreen.tsx` — the new section (node 1 owns the rest of the row).
- `src/gen/connection_pb.ts` — regenerated.

## Implementation milestones

- [ ] Proto + both regenerations
- [ ] Probe trait + injectable double; workspace builds
- [ ] git identity probe, all three outcomes
- [ ] `gh` probe, all three outcomes
- [ ] Timeout behaviour proven
- [ ] Handler auth + peer routing
- [ ] Row section rendering every state distinctly
- [ ] Supervisor allowlist verified on a real deployment shape

## Testing plan

**Levels.**

| Level | Where | Why |
|---|---|---|
| Unit (Rust) | `packages/tddy-daemon/src/host_tooling.rs` `#[cfg(test)]` | Parsing `gh auth status` output and classifying outcomes is pure logic and must be pinned per outcome |
| Integration (Rust) | `packages/tddy-daemon/src/connection_service.rs` `#[cfg(test)]` | Auth rejection and the injected-probe path only exist at the handler |
| Component (Cypress) | `packages/tddy-web/cypress/component/` | Six states rendering distinguishably is a UI contract, and the one an operator is misled by |

**Options considered.**

| Option | Verdict |
|---|---|
| Inject a `HostTooling` double and assert per outcome | **Chosen.** Deterministic, no dependency on what is installed on the CI machine |
| Shell out to the real `git` / `gh` in tests | Rejected — CI has an arbitrary `gh` state; the suite would be environment-dependent, and asserting against the developer's own identity is exactly the anti-pattern the testing guide names |
| A test-only branch in the probe | **Forbidden** by CLAUDE.md — no code branches that only work in test |

**Parsing note.** `gh auth status` writes to **stderr** and its wording is not a stable API. The parse
must be narrow, and "output we do not recognise" must classify as **probe failed**, not as logged out —
misreporting an authenticated host as logged out is the worse error.

**The gap this plan cannot close locally.** Whether `git` and `gh` are resolvable under
`resolve_tool_path` and whether the probe env survives `resolve_env`
(`packages/tddy-supervisor/src/policy.rs:96`, `:124` — an **allowlist**) cannot be proven by a unit
test. Flag it explicitly in the PR and verify on a supervised deployment.

**Coverage.** Every AC maps to a named test.

## Acceptance tests

**`packages/tddy-daemon/src/host_tooling.rs`** (unit)

| Test | Validates |
|---|---|
| `reports_the_configured_git_user_name_and_email` | AC-1 |
| `reports_that_no_git_identity_is_configured_rather_than_an_empty_name` | AC-2 |
| `reports_that_the_github_cli_is_not_installed` | AC-3 |
| `reports_the_github_cli_as_logged_out` | AC-4 |
| `reports_the_login_the_github_cli_is_authenticated_as` | AC-5 |
| `classifies_unrecognised_gh_output_as_a_probe_failure_not_as_logged_out` | AC-6 |
| `reports_a_probe_failure_when_the_lookup_exceeds_its_timeout` | AC-6 |

**`packages/tddy-daemon/src/connection_service.rs`** (integration)

| Test | Validates |
|---|---|
| `host_tooling_rejects_an_invalid_token` | AC-8 |
| `host_tooling_runs_the_probe_as_the_hosts_os_user` | AC-7 |
| `host_tooling_for_another_host_is_routed_to_that_peer` | AC-9 |

**`packages/tddy-web/cypress/component/HostsScreenToolingAcceptance.cy.tsx`**

| Test | Validates |
|---|---|
| `shows_the_git_identity_a_host_is_configured_with` | AC-1 |
| `distinguishes_a_host_with_no_git_identity_from_one_that_failed_to_report` | AC-2, AC-6 |
| `distinguishes_gh_absent_from_gh_logged_out_from_gh_authenticated` | AC-3, AC-4, AC-5 |
| `labels_the_gh_login_as_the_hosts_rather_than_the_signed_in_user` | AC-10 |

## Decisions & trade-offs

- **Two fixed probes, not a general remote-exec RPC.** A session-less "run this on host X" primitive
  would be the largest security surface in the stack, and nothing here needs it.
- **`run_capture_as_user`, not the `Shell` tool.** The tool path requires a `session_id`, and the
  user-scoped `HOME`/`PATH` handling is exactly what makes these probes correct.
- **This node owns the probe RPC shape** for nodes 5 and 7. One RPC that grows blocks beats three
  near-identical RPCs — but it does mean nodes 5 and 7 extend a message this node owns, which their
  `## Dependencies` records.
- **Unrecognised output is a failure, not a negative.** Reporting an authenticated host as logged out
  is worse than admitting the probe did not understand the answer.
- **Six states on the wire.** Anything collapsed client-side becomes a fabricated value.

## Technical debt & production readiness

- ⚠ **Supervisor allowlist unverified at planning time** — see the testing plan. Must be checked before
  this PR is set ready for review.
- ⚠ `run_capture_as_user` is Unix-only; the non-Unix path bails. The probe must report a clean
  "unsupported platform" rather than surfacing an internal error.

## Refactoring needed

_(populated by each validation phase)_

## Validation results

_(populated by each validation command)_

## TODO

- [x] Record initial discovery (`2026-09-06-host-identity-initial-discovery.md`)
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

- [`2026-09-06-agent-keys.md`](./2026-09-06-agent-keys.md) — branch `feature/hosts-screen/agent-keys`
