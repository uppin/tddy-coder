# Changeset: host-identity

**Date:** 2026-09-06
**Status:** ✅ Implemented — pending user review
**Type:** New feature
**Stack:** `#hosts-screen` 4/8
**Branch:** `feature/hosts-screen/host-identity` → base `feature/hosts-screen/host-resources`
**PR:** [#456](https://github.com/uppin/tddy-coder/pull/456)

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
  `spawner::start_output_as_user`, as the host's OS user, with a bounded timeout that kills and reaps
  an overrunning child rather than abandoning it.
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

## Prerequisites

Open items in [`docs/dev/TODO.md`](../TODO.md) this PR runs into.

### ⚠ DURING — `connection_service.rs` is 19,600 lines (recorded); **22,452 today**

`docs/dev/TODO.md` § *`connection_service.rs` is 19,600 lines* (source:
subagent-conversation-inference, 2026-08-29), flagged rather than acted on. The recorded figure is
stale — the file is **22,452 lines** as of this PR, ~15% past it. Not corrected in `docs/dev/TODO.md`
itself: node 3 of this stack also edits that file, and a shared-file edit for a number is not worth
the merge conflict.

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

- [x] Probe RPC + messages, both regenerations
- [x] Probe module with an injectable seam and a bounded timeout
- [x] git identity probe (configured / not configured / failed)
- [x] `gh` probe (absent / logged out / authenticated as)
- [x] Handler with auth and peer routing
- [x] Tooling section on the Hosts row — as a self-contained component; node 1's row mounts it (see Delta)
- [x] Rust unit/integration tests + Cypress component tests
- [x] ~~Verify behaviour under the supervisor's tool/env allowlist~~ — **the allowlist was never on this
  path.** Replaced by the two findings in *Technical debt*: the daemon's own `resolve_tool_path` (found
  and fixed) and the unprivileged-daemon `setuid` limit (open, needs a deployment decision).

## Technical changes

### State A

- **git identity:** no production reader. Every `user.name` / `user.email` hit is a test fixture
  writing into a temp repo. No git-config abstraction exists; `DaemonConfig::git`
  (`config.rs:191-201`) holds only `ssh_command`.
- **`gh`:** absent from product code entirely. `grep` for `gh auth` / `Command::new("gh")` over
  `packages/**` returns zero. `packages/tddy-github` is an OAuth login crate, not a `gh` wrapper.
- **Running a command as the host user:** `spawner::run_capture_as_user` exists and handles
  `HOME` / `PATH` / privilege drop. Unix only; the non-Unix path bails. It returns stdout only and
  collapses every other outcome into an error string — see *Delta* for why that was not usable here.
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
- `src/host_tooling.rs`: the probe trait, its real implementation over `start_output_as_user`, the
  deadline (kill + reap), and the outcome types.
- `src/connection_service.rs`: handler, auth, peer routing, an injected `Arc<dyn HostTooling>` field
  and a builder override for tests (mirroring `with_host_stats`, `:2016`).
- `src/connection_tonic_adapter.rs`: the unary adapter entry.

**`packages/tddy-web`**
- `src/components/hosts/HostRowTooling.tsx` — the row section and its states.
- `src/gen/connection_pb.ts` — regenerated.

**`HostsScreen.tsx` is deliberately not touched here** — planning listed it, implementation did not,
and the plan was wrong. Node 1 owns the Hosts row; mounting this section into it from node 4 would
edit a file that node's PR owns, for a row whose rendering is still theirs to finish. So this node
ships the section as a self-contained component with its own acceptance tests, and node 1's row
mounts it.

The consequence is worth stating plainly rather than leaving a reader to discover it: **no code path
in the repo issues `GetHostTooling` yet.** The RPC, the probe and the component are each covered by
their own tests, but the feature is not reachable from the UI until the row mounts the section, so
AC-1 – AC-6 and AC-10 are proven per-unit and not end-to-end. That is a sequencing fact of the stack,
not an omission in this node.

## Implementation milestones

- [x] Proto + both regenerations
- [x] Probe trait + injectable double; workspace builds
- [x] git identity probe, all three outcomes
- [x] `gh` probe, all three outcomes
- [x] Timeout behaviour proven — the child is now killed and reaped on deadline, not abandoned
- [x] Handler auth + peer routing — routing precedes auth, matching the file's documented contract
- [x] Row section rendering every state distinctly
- [ ] ⚠ **Unprivileged-daemon probe of another OS user** — open; not verifiable locally, and AC-7 does
  not hold on the `--systemd` deployment shape. See *Technical debt*.

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

Restructured during green: the plan's four specs mounted several states per `it()` and asserted only
that each state's own string was present. That is not enough to prove the word *distinguishes* —
a component collapsing "authenticated" into `Not authenticated (octocat)` passed every one of them.
Now one behaviour per test, and each state also **denies** the neighbouring state it must not be
confused with.

| Test | Validates |
|---|---|
| `shows the git identity a host is configured with` | AC-1 |
| `reports a host with no git identity as not configured` | AC-2 |
| `distinguishes a host with no git identity from one that failed to report` | AC-2, AC-6 |
| `reports a host without the github cli as not installed` | AC-3 |
| `reports a host with the github cli logged out as not authenticated` | AC-4 |
| `reports the login the github cli on a host is authenticated as` | AC-5 |
| `labels the gh login as the hosts rather than the signed in user` | AC-10 |
| `says nothing about a host that has not answered yet` | the pre-answer state |

⚠ **The AC-10 spec as planned was a false positive.** It asserted the `gh` cell contained the string
`"gh"` — which is the static label rendered in *every* state, for any login or none. It passed
whatever the component did. It now asserts the `title` attribute names the host and carries the
login, and fails if either is dropped.

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

- ✅ **The supervisor allowlist was the wrong thing to worry about — resolved, and the plan corrected.**
  Planning flagged `resolve_tool_path` / `resolve_env` in `packages/tddy-supervisor/src/policy.rs` as
  the likeliest "works locally, fails on a real deployment" gap. Neither is on this path:
  `spawner::run_output_as_user` forks and drops privilege itself (`setgid` / `initgroups` / `setuid`
  in `pre_exec`), with no supervisor brokering, so there is no allowlist for the probe to fail.

  What the flag *should* have said, and what green actually found, is that the daemon has its **own**
  `resolve_tool_path` (`spawner.rs`), which anchors a relative program name to the daemon's process
  cwd and never searches `PATH`. Bare `"git"` / `"gh"` therefore resolved to `<daemon-cwd>/git` — and
  under `./install --systemd` the unit sets no `WorkingDirectory=`, so that is `/git`. The probe could
  not have run on any deployment. Worse, `gh`'s `ErrorKind::NotFound` was mapped to
  `installed: false`, so **every host would have reported "gh is not installed"** — precisely the
  fabricated negative this node's whole design exists to prevent. Fixed by resolving both programs on
  the same `PATH` the child is given (`spawner::find_program_on_spawn_child_path`), and by requiring
  positive evidence from that lookup before "not installed" may be claimed.

  This was provable locally by inspection all along. It was missed because the one acceptance test
  that would have exec'd anything — `host_tooling_runs_the_probe_as_the_hosts_os_user` — had not been
  written.

- ⚠ **An unprivileged daemon cannot probe another OS user.** `run_output_as_user`'s privilege drop
  calls `libc::setgid` / `setuid` directly, which requires privilege. Under `./install --systemd` the
  daemon runs as an unprivileged child of `tddy-supervisor`, so for any `os_user` other than the
  daemon's own the `pre_exec` returns `EPERM` and the probe reports `Failed` — honestly, but AC-7 is
  then unsatisfiable on that deployment shape. **Pre-existing** for `run_capture_as_user`, whose one
  caller has the same constraint; this node is simply the first to exercise it on a per-host,
  multi-user path. Needs a decision on a supervised deployment, and it is not this node's to make.

- ⚠ `run_output_as_user` is Unix-only; the non-Unix path bails. The probe reports a clean
  `ProbeOutcome::Unsupported` rather than surfacing an internal error.

- ⚠ **No cache and no in-flight dedup.** The PRD's model is the Hosts screen probing every host it
  lists, i.e. a poll, and overlapping calls stack rather than coalescing. The neighbouring
  `list_agent_models` caches per `(os_user, daemon, agent)` for exactly this reason. Latent while the
  row does not mount the section; worth resolving before it does.

- 📏 `connection_service.rs` is now **22,452 lines**, against the ~19,600 recorded in
  `docs/dev/TODO.md` (2026-08-29). Not corrected there: node 3 of this stack also edits that file and
  a shared-file edit for a figure is not worth the conflict. Recorded here so the number is not read
  as current.

## Refactoring needed

Raised by validation and **fixed in this PR**:

- `spawner.rs` — the setup shared by `run_output_as_user` and the new `start_output_as_user`
  (`resolve_tool_path`, `getpwnam_r`, `HOME`/`PATH`, `current_dir`, the privilege drop) extracted into
  `as_user_command` rather than duplicated.
- `run_capture_as_user` reduced to a thin wrapper over `run_output_as_user`, signature and error
  strings byte-identical, because its one existing caller (`connection_service.rs`, `list_agent_models`)
  and its pinned test must not move.
- `HostRowTooling` — the `unanswered` guard inverted so only `ProbeOutcome.OK` licenses a finding.
  proto3 enums are open and nodes 5 and 7 extend this message, so listing the *no-finding* outcomes
  would let an unknown value from a newer daemon render "Not configured" with full confidence.

**Deferred, deliberately:** `connection_service.rs` is not split. Nodes 1, 3, 4 and 6 of this stack all
edit it, and restructuring a file a parent and a dependent both touch turns every one of their diffs
into a conflict. It needs its own PR after the stack lands — see *Prerequisites*.

## Validation results

### Red phase (draft-PR contract)

- `cargo build -p tddy-service` / `-p tddy-daemon --tests` — pass; TypeScript regenerated.
- `host_tooling.rs` — **6 tests, 6 red**, all at the two `unimplemented!()` classifiers.
- **`classify_gh_auth_status` is a pure function of `(exit_code, output)`**, split out from the
  subprocess call. The classification is the part that is easy to get wrong and expensive to get
  wrong, and this makes it provable without spawning anything or depending on whatever `gh` happens
  to be installed on the machine running the suite.
- The Cypress spec asserts the six states are **distinguishable**, not merely present: the
  "failed to report" case asserts it does *not* also read as "Not configured". A per-state test in
  isolation would pass even if the component collapsed two states into one rendering — which is
  exactly the bug that sends an operator to configure git on a host where git is not installed.

### Green phase

**Two blockers found by validation, both fixed.** Neither was visible in a test run, and both were
provable by inspection — which is the finding worth keeping.

1. **The probes could never execute.** `host_tooling.rs` passed bare `"git"` / `"gh"` to the spawner,
   whose `resolve_tool_path` anchors a *relative* program name to the daemon's own process cwd and
   never searches `PATH` (deliberate, and pinned by
   `run_capture_as_user_locates_a_relative_program_path_against_the_daemons_own_cwd_not_the_target_users_home`).
   So the daemon exec'd `<daemon-cwd>/git`; under `./install --systemd`, which sets no
   `WorkingDirectory=`, that is `/git`. Fixed with `spawner::find_program_on_spawn_child_path`, which
   resolves against the same `PATH` the child is given.
2. **`gh` reported a fabricated negative.** `ErrorKind::NotFound` from `output()` was mapped to
   `installed: false`, so **every host would have reported "gh is not installed"** — including hosts
   with an authenticated `gh`. `NotFound` on that path also covers a missing `current_dir` and an
   unmounted home, so it can never be evidence of absence. "Not installed" now requires a positive
   result from the `PATH` lookup; anything else is `Failed`. This is precisely the collapse the module
   was written to prevent, and it had shipped inside it.

Also corrected: routing now precedes authentication (a relay must not judge a peer's user mapping —
the contract `rpc_served_by_peer` and `resolve_stack_base` already document); `git config --global
--get`, so the code matches its documented "reads `$HOME/.gitconfig`" contract; `record_rpc_activity()`;
and a `log::warn!` on every `Failed` outcome, without which blocker 1 was invisible in the daemon log.

**The timeout leak is fixed, not just documented.** A timed-out probe previously abandoned its child
*and* its reader thread — `spawner` handed back no kill handle. `start_output_as_user` now returns the
live `Child`; the reader thread gets only the pipes, and the deadline `kill()`s **and** `wait()`s, since
kill alone leaves a zombie.

**Test evidence.** 7 unit + 3 integration + 8 Cypress, all green. Both new-test authors mutation-checked
their work rather than trusting a green run:

| Mutation | Result |
|---|---|
| `probe(&probed_user)` → `probe("")` | AC-7 fails: `left: [""], right: ["ada"]` — the exact blocker shape |
| auth moved back above routing | AC-9 fails: `PermissionDenied` instead of `FailedPrecondition` |
| `running.end()` → `drop(running)` | AC-6 fails: "the overrunning child must be ended and reaped" |

**Why the blockers reached green at all:** every one of the four acceptance tests the plan named but
red never wrote (AC-6, AC-7, AC-8, AC-9) sat on the path that broke, and
`host_tooling_runs_the_probe_as_the_hosts_os_user` is specifically the one that would have exec'd
something. The seam built to make it writable — `with_host_tooling` — sat unused. All four now exist.

**Known limit of the AC-9 test.** `rpc_served_by_peer` calls `forward_to_peer` with a live room slot
and has no seam beneath it, so a unit test cannot receive a real peer's response. The test pins that
the handler *reaches* forwarding, that it does so before local auth, and that the local probe was never
called — not the wire round-trip, which needs a LiveKit integration test.

## TODO

- [x] Record initial discovery (`2026-09-06-host-identity-initial-discovery.md`)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [x] Repeat Red→Green→Update cycle until feature complete
- [x] Run all tests — `./test -p tddy-daemon` + the Cypress spec, scoped to the packages this node
  touches (a full-workspace run carries pre-existing noise that reads as damage from this PR)
- [x] Validate changes (/validate-changes)
- [x] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [x] Validate tests (/validate-tests)
- [x] Refactor test issues — the vacuous AC-10 assertion and the missing negatives
- [x] Validate production readiness (/validate-prod-ready)
- [x] Refactor production readiness issues — both blockers, plus the timeout leak
- [x] Analyze code quality (/analyze-clean-code)
- [x] Refactor code quality issues — the `connection_service.rs` split stays deferred; see
  *Refactoring needed*
- [x] Final validation (/validate-changes)
- [x] Linting and formatting (`cargo clippy -- -D warnings`, `cargo fmt`)
- [x] Wrap documentation (/wrap-context-docs)
- [ ] USER REVIEW — work complete, decide next steps

**Open, and not this node's to close:** the unprivileged-daemon `setuid` limit means AC-7 does not hold
on the `./install --systemd` deployment shape. See *Technical debt & production readiness*.

## Successor PRs

- [`2026-09-06-agent-keys.md`](./2026-09-06-agent-keys.md) — branch `feature/hosts-screen/agent-keys`
