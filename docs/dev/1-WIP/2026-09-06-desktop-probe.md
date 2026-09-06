# Changeset: desktop-probe

**Date:** 2026-09-06
**Status:** 🚧 In Progress
**Type:** New feature
**Stack:** `#hosts-screen` 7/8
**Branch:** `feature/hosts-screen/desktop-probe` → base `feature/hosts-screen/agent-add-key`

## Initial Discovery

[`./2026-09-06-desktop-probe-initial-discovery.md`](./2026-09-06-desktop-probe-initial-discovery.md)

## Related feature documentation

PRD: [`docs/ft/web/1-WIP/PRD-2026-09-06-desktop-probe.md`](../../ft/web/1-WIP/PRD-2026-09-06-desktop-probe.md)

Affected: [`screen-sharing-sessions.md`](../../ft/web/screen-sharing-sessions.md)

## Affected packages

| Package | Change |
|---|---|
| [`packages/tddy-service`](../../../packages/tddy-service) | remote-desktop block on the host tooling response |
| [`packages/tddy-daemon`](../../../packages/tddy-daemon) | reachability probe, bridge-binary existence check |
| [`packages/tddy-web`](../../../packages/tddy-web) | remote-desktop section on the Hosts row |

## Responsibility

- A bounded, non-intrusive TCP reachability probe for the VNC and RDP ports.
- An **existence check on the resolved bridge binary path**, converting a post-hoc spawn failure into a
  fact reported up front.
- Reporting the two facts — bridge capability and desktop reachability — **separately**, plus the port
  that was checked, and a distinct probe-failure state.
- The remote-desktop section on each Hosts row.

## Boundaries

- Does **not** start a stream, spawn a bridge, or open a viewer. Node 8 owns all of that.
- Does **not** create a host-scoped target or vault model — node 8's.
- Does **not** change `ScreenSharingService`, its per-session targets, its vault, or its spawn path.
- Does **not** change `resolve_vnc_binary_path` / `resolve_rdp_binary_path`'s resolution **order** — it
  only checks whether what they resolve to exists.
- Does **not** define a second `Protocol` enum; it reuses `screen_sharing.proto:19-23`.
- Does **not** perform a protocol handshake against a remote desktop.
- Does **not** discover non-default ports.
- Does **not** touch nodes 1–6's surfaces.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `host-identity` (#hosts-screen 4/8) | the tooling probe RPC, its response message, the injectable probe seam, and `HostRowTooling` | **adds a remote-desktop block** to that response and a section to that row component | rename, restructure or renumber node 4's messages; change the git/`gh` probes; create a competing RPC |
| `agent-keys` (#hosts-screen 5/8) | the ssh-agent block on the same response | shares the message — must not race its field numbering | edit the ssh-agent block |
| `host-registry` (#hosts-screen 1/8) | rows with `instance_id` and `online` | probes only online hosts | change the registry |

> ⚠ **Shared-file hazard.** Nodes 3–7 all add to `connection.proto`, and 4, 5 and 7 all add blocks to
> the *same* response message. Pick genuinely free field numbers; a conflict resolves by **renumbering**,
> never by taking one side.

## Draft PR contract

Lands first, so node 8 can branch off a real ref:

1. The remote-desktop block on the tooling response, regenerated in both languages — the shape node 8
   reads to decide whether to offer a connect action.
2. The probe module's seam, so a double can be injected.
3. The failing tests below.

## Summary

Report whether a host has a reachable remote desktop, and whether its daemon can bridge one at all.
tddy already bridges VNC and RDP, but has no notion of availability — not even an `exists()` check on
the bridge binary, whose absence currently surfaces only as a spawn error after the user has asked for
a stream.

## Scope

- [ ] Bridge-binary existence check for both protocols
- [ ] Bounded, non-intrusive TCP reachability probe
- [ ] Remote-desktop block on node 4's response + both regenerations
- [ ] Row section presenting the two facts separately
- [ ] Rust unit/integration tests + Cypress component tests

## Technical changes

### State A

- `packages/tddy-vnc` (RFB via the `vnc-rs` git dependency) and `packages/tddy-rdp` (IronRDP) are
  outbound bridge binaries reading a JSON `BridgeConfig` from **stdin** ("to avoid exposing credentials
  in argv/ps") and republishing a framebuffer as a LiveKit video track.
- `ScreenSharingService` (`screen_sharing.proto:8-15`) drives them, **per session**: every request
  carries `session_token` + `session_id`, and the vault lives under the session dir
  (`screen_sharing_vault.rs`). Legacy `vnc.proto:7-13` still exists alongside.
- `enum Protocol { UNSPECIFIED, VNC, RDP }` at `screen_sharing.proto:19-23`.
- ⚠ `resolve_vnc_binary_path` / `resolve_rdp_binary_path` (`config.rs:451-500`) **guess** a path —
  config → sibling of `current_exe()` → bare name on `PATH` — with **no existence check**. A missing
  binary appears only as a spawn `error!` at `screen_sharing_service.rs:174`.
- No reachability probe of any kind exists.
- The web viewer exists (`ScreenSharingOverlay.tsx`, a LiveKit `VideoTrack`); there is **no browser-side
  VNC/RDP protocol client** — `package.json` has no novnc/guacamole/rfb.

### State B

- The daemon reports, per host and per protocol: whether the bridge binary exists, whether the port
  accepts a connection, which port was checked, and whether the probe itself failed.
- Each Hosts row presents bridge capability and desktop reachability as two separate facts.

### Delta

**`packages/tddy-service`**
- `connection.proto`: a remote-desktop block on node 4's tooling response — per protocol, a
  reachability state, a bridge-availability flag and the checked port. Reuses `Protocol`.

**`packages/tddy-daemon`**
- `src/remote_desktop_probe.rs`: the probe trait, a TCP-connect implementation with a timeout, the
  bridge-binary existence check, and the outcome types.
- `src/host_tooling.rs`: populate the block (node 4 owns the module).
- `src/connection_service.rs`: injectable probe seam for tests.

**`packages/tddy-web`**
- `src/components/hosts/HostRowRemoteDesktop.tsx` — the section and its states.
- `src/components/hosts/HostRowTooling.tsx` — mount it (node 4 owns the component).

## Implementation milestones

- [ ] Bridge existence check for both protocols
- [ ] TCP probe with timeout, proven non-intrusive
- [ ] Block on the wire + both regenerations
- [ ] Row section presenting both facts distinctly
- [ ] `./test -p tddy-daemon` and touched Cypress specs green

## Testing plan

**Levels.**

| Level | Where | Why |
|---|---|---|
| Unit (Rust) | `packages/tddy-daemon/src/remote_desktop_probe.rs` `#[cfg(test)]` | Reachability against a **real ephemeral listener** on `127.0.0.1` is deterministic and needs no network |
| Integration (Rust) | `packages/tddy-daemon/src/host_tooling.rs` `#[cfg(test)]` | The block only reaches the wire through the probe |
| Component (Cypress) | `packages/tddy-web/cypress/component/` | Not conflating the two facts is a UI contract |

**Options considered.**

| Option | Verdict |
|---|---|
| Bind an ephemeral `TcpListener` on `127.0.0.1:0` and probe it | **Chosen.** Real socket behaviour, hermetic, no fixed ports |
| Probe a real VNC server in CI | Rejected — the CI gate deliberately excludes VM-backed and desktop suites; this would be flaky and slow |
| Mock the probe trait only | Used **in addition**, at the handler level; alone it would never exercise the timeout |

**Non-intrusiveness is asserted, not assumed.** The listener records what it received: the test proves
the probe wrote **no bytes** before closing. Otherwise "we don't handshake" is a comment, not a fact.

**A closed port must be distinguished from a timeout.** Connecting to an unbound `127.0.0.1` port
refuses immediately; a black-holed address times out. Both are pinned, because AC-5's whole point is
that they are different answers.

**Coverage.** Every AC maps to a named test.

## Acceptance tests

**`packages/tddy-daemon/src/remote_desktop_probe.rs`** (unit)

| Test | Validates |
|---|---|
| `reports_a_port_with_a_listener_as_reachable` | AC-1, AC-2 |
| `reports_a_port_with_no_listener_as_unreachable_naming_the_port` | AC-3 |
| `reports_a_probe_failure_rather_than_unreachable_when_the_connect_times_out` | AC-5 |
| `writes_no_bytes_to_the_remote_before_closing` | AC-6 — the rudeness guard |
| `reports_that_the_host_cannot_bridge_when_the_binary_is_missing` | AC-4 |

**`packages/tddy-daemon/src/host_tooling.rs`** (integration)

| Test | Validates |
|---|---|
| `host_tooling_reports_bridge_availability_separately_from_reachability` | AC-4, AC-7 |
| `host_tooling_still_reports_git_gh_and_the_ssh_agent_alongside_the_desktop_block` | no regression on nodes 4–5 |

**`packages/tddy-web/cypress/component/HostsScreenRemoteDesktopAcceptance.cy.tsx`**

| Test | Validates |
|---|---|
| `shows_vnc_and_rdp_reachability_for_a_host` | AC-1, AC-2 |
| `distinguishes_a_host_that_cannot_bridge_from_one_with_no_desktop_serving` | AC-4, AC-7 |
| `names_the_port_that_was_checked_when_reporting_unreachable` | AC-3 |

## Decisions & trade-offs

- **Two facts, never one.** Conflating "tddy can't bridge here" with "nothing is serving here" sends an
  operator to the wrong fix.
- **TCP connect, no handshake.** A monitoring screen polling half-open RFB handshakes against people's
  desktops is antisocial; the connect answers the question asked.
- **Default ports only, and say so.** Reporting the checked port keeps "unavailable" from reading as
  authoritative for a host on a non-standard port. Discovery is out of scope.
- **Reuse `Protocol`.** Two enums meaning the same thing drift apart.
- **The `exists()` check is the cheapest win here** — it turns an error the user meets *after* asking
  for a stream into a fact they see beforehand.

## Technical debt & production readiness

- ⚠ Probing on every screen refresh costs a TCP connect per host per protocol. If a fleet makes that
  noticeable, cache with a short TTL — recorded, not pre-optimised.

## Refactoring needed

_(populated by each validation phase)_

## Validation results

_(populated by each validation command)_

## TODO

- [x] Record initial discovery (`2026-09-06-desktop-probe-initial-discovery.md`)
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

- [`2026-09-06-desktop-connect.md`](./2026-09-06-desktop-connect.md) — branch
  `feature/hosts-screen/desktop-connect`
