# Changeset: desktop-connect

**Date:** 2026-09-06
**Status:** 🚧 In Progress
**Type:** New feature
**Stack:** `#hosts-screen` 8/8 — the top node
**Branch:** `feature/hosts-screen/desktop-connect` → base `feature/hosts-screen/desktop-probe`

## Initial Discovery

[`./2026-09-06-desktop-connect-initial-discovery.md`](./2026-09-06-desktop-connect-initial-discovery.md)

## Related feature documentation

PRD: [`docs/ft/web/1-WIP/PRD-2026-09-06-desktop-connect.md`](../../ft/web/1-WIP/PRD-2026-09-06-desktop-connect.md)

Affected: [`screen-sharing-sessions.md`](../../ft/web/screen-sharing-sessions.md)

## Affected packages

| Package | Change |
|---|---|
| [`packages/tddy-service`](../../../packages/tddy-service) | host-scoped desktop target + start/stop RPCs |
| [`packages/tddy-daemon`](../../../packages/tddy-daemon) | host-scoped target store, host-scoped bridge start/stop |
| [`packages/tddy-web`](../../../packages/tddy-web) | connect action, host-scoped overlay mount, capability gating |

## Responsibility

- A **host-scoped desktop target model** (label, host, port, protocol, username) and its storage,
  separate from the session-scoped vault.
- Host-scoped **start/stop**, reusing the existing bridge spawn path and returning the same response
  shape the overlay consumes.
- The **connect action** on the host row, gated on node 7's reachability **and** the `media` capability
  — removed, not disabled, where media is absent.
- Mounting the existing `ScreenSharingOverlay` at host scope, with input forwarding.
- Prompting for a desktop password through **node 6's encrypted channel**, without persisting it.

## Boundaries

- Does **not** implement a VNC or RDP protocol client in the browser. Rendering is always a
  daemon-produced LiveKit video track. This is a permanent boundary, not a scoping choice.
- Does **not** change `packages/tddy-vnc`, `packages/tddy-rdp`, or the bridge spawn path in
  `screen_sharing_service.rs:163-176`.
- Does **not** change `ScreenSharingOverlay`, `vncInput.ts`, or the per-session tabs — it reuses them.
- Does **not** change the per-session target model, its vault, or `UnlockVault`.
- Does **not** change node 7's probe, node 6's prompt channel, node 5's agent client, node 4's probes,
  node 3's telemetry, node 2's fan-out, or node 1's registry.
- Does **not** discover non-default ports or offer multi-monitor selection.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `desktop-probe` (#hosts-screen 7/8) | the remote-desktop block on the tooling response — reachability, bridge availability, checked port | offers the connect action only where the desktop is reachable and the host can bridge | change the probe, its ports, or its block's shape |
| `agent-add-key` (#hosts-screen 6/8) | `StreamHostPrompts` + `AnswerHostPrompt`, the host keypair, browser-side `SubtleCrypto` encryption and the key-pinning module | reuses the channel to prompt for a desktop password without persisting it | change the prompt channel, the crypto, the keypair lifecycle or the pinning rules |
| `host-registry` (#hosts-screen 1/8) | rows with `instance_id` and `online` | addresses the connect at a host | change the registry |

> ⚠ **Sequencing.** This node consumes node 6's prompt channel *and* node 7's probe block. Both must
> exist at `HEAD` before `/green`. Rebase onto the parent (`/pr-stack-rebase`) first.

## Draft PR contract

This is the **top node** — nothing branches off it. Its first push is still the API surface plus
failing tests, so the PR is reviewable early:

1. The host-scoped target messages and start/stop RPCs, regenerated in both languages.
2. The host-scoped target store's seam.
3. The failing tests below.

## Summary

Open a host's desktop from the Hosts screen, in the overlay tddy already has. The bridges, the overlay
and the input channel all exist; what does not is **host scope** — every `ScreenSharingService` request
carries a `session_id` and the credential vault lives under the session directory. A desktop belongs to
a machine, not to a coding session.

## Scope

- [ ] Host-scoped target model + storage
- [ ] Host-scoped start/stop RPCs + both regenerations
- [ ] Bridge start/stop at host scope, reusing the existing spawn path
- [ ] Connect action, gated on reachability **and** `media`
- [ ] Overlay mounted at host scope with input forwarding
- [ ] Desktop password via node 6's encrypted prompt, not persisted
- [ ] Rust unit/integration tests + Cypress component tests

## Technical changes

### State A

- Bridges exist and work: `packages/tddy-vnc` (RFB via `vnc-rs`), `packages/tddy-rdp` (IronRDP), both
  reading a JSON `BridgeConfig` from **stdin** so credentials never reach argv.
- `ScreenSharingService` (`screen_sharing.proto:8-15`) drives them **per session**; every request
  carries `session_token` + `session_id`; the vault is under the session dir.
- `StartStreamResponse:100-107` returns `{ livekit_room, livekit_url, bridge_identity, track_name,
  width, height }` — already exactly the overlay's props.
- Spawn + PID tracking: `screen_sharing_service.rs:163-176`, `active_bridges` for `stop_stream`.
- `ScreenSharingOverlay.tsx:1-35` renders a bridge participant's LiveKit `VideoTrack`; `vncInput.ts`
  forwards input. **No browser-side protocol client exists** — no novnc/guacamole/rfb in `package.json`.
- ⚠ `InspectorTabs.tsx:101-110` **removes** the tabs without the `media` capability;
  `IPC_CAPABILITIES = {"rpc"}` (`localHost.ts:59`) — a frame pipe carries no media.

### State B

- A desktop target can belong to a host; the daemon starts and stops a bridge at host scope.
- The Hosts row offers a connect action where the desktop is reachable and the wire carries media, and
  the existing overlay renders that host's desktop.
- A required password is prompted through node 6's encrypted channel and dropped after use.

### Delta

**`packages/tddy-service`**
- `connection.proto` (or a host-scoped section of `screen_sharing.proto` — decided in the red phase,
  recorded here as open): host-scoped target messages and start/stop RPCs. Reuses `Protocol`.

**`packages/tddy-daemon`**
- `src/host_desktop_targets.rs`: the host-scoped target store.
- `src/screen_sharing_service.rs` or a host-scoped sibling: start/stop at host scope, **calling** the
  existing spawn path rather than duplicating it.

**`packages/tddy-web`**
- `src/components/hosts/HostRowRemoteDesktop.tsx` — the connect action (node 7 owns the section).
- `src/components/hosts/HostDesktopOverlay.tsx` — a thin host-scoped mount of the existing overlay.

## Implementation milestones

- [ ] Target model + store, unit-tested
- [ ] Host-scoped start returning overlay-ready values
- [ ] Overlay mounted, a real track rendering
- [ ] Input forwarding verified
- [ ] Stop releasing the bridge process
- [ ] Capability gating removing the action without media
- [ ] Password prompt through node 6's channel, nothing persisted

## Testing plan

**Levels.**

| Level | Where | Why |
|---|---|---|
| Unit (Rust) | `packages/tddy-daemon/src/host_desktop_targets.rs` `#[cfg(test)]` | Host/session target isolation is a storage property and must be provable directly |
| Integration (Rust) | `packages/tddy-daemon/src/screen_sharing_service.rs` `#[cfg(test)]` | Start/stop lifecycle and PID release only exist at the service |
| Component (Cypress) | `packages/tddy-web/cypress/component/` | The gating rule — action **absent**, not disabled — is a UI contract |

**Options considered.**

| Option | Verdict |
|---|---|
| Fake bridge binary + in-memory RPC backend | **Chosen.** A stub binary that exits on stdin close proves spawn and release without a real desktop |
| Real VNC server end-to-end | Rejected — the CI gate excludes VM-backed and desktop suites; this belongs to manual verification, noted in the PR |
| Assert the overlay renders real pixels | Rejected — that is LiveKit's contract, already covered by the per-session specs |

⚠ **The pieces this node reuses must be proven still working.** The existing per-session screen-sharing
specs must stay green (AC-9); a regression there is more likely than a bug in the new code, because
host scope touches the same service.

**Manual verification, stated rather than skipped.** Actual pixels from a real VNC/RDP server cannot be
asserted in the CI gate. The PR must record that this was exercised by hand, on which protocol, and
against what.

**Coverage.** Every AC maps to a named test, except AC-3's real-pixel path, which is the manual item.

## Acceptance tests

**`packages/tddy-daemon/src/host_desktop_targets.rs`** (unit)

| Test | Validates |
|---|---|
| `a_host_scoped_target_is_not_visible_to_the_session_scoped_store` | AC-8 |
| `a_session_scoped_target_is_not_visible_to_the_host_scoped_store` | AC-8 |

**`packages/tddy-daemon/src/screen_sharing_service.rs`** (integration)

| Test | Validates |
|---|---|
| `starting_a_host_desktop_returns_the_room_identity_and_track_the_viewer_needs` | AC-2 |
| `stopping_a_host_desktop_releases_the_bridge_process` | AC-4 |
| `a_host_desktop_password_never_appears_in_the_bridge_process_arguments` | AC-10 |
| `starting_a_session_desktop_still_works_unchanged` | AC-9 |

**`packages/tddy-web/cypress/component/HostDesktopConnectAcceptance.cy.tsx`**

| Test | Validates |
|---|---|
| `offers_a_connect_action_for_a_reachable_host_on_a_media_carrying_connection` | AC-1 |
| `omits_the_connect_action_entirely_when_the_connection_carries_no_media` | AC-5 |
| `omits_the_connect_action_when_the_desktop_is_unreachable` | AC-6 |
| `opens_the_overlay_for_the_selected_host` | AC-2 |
| `prompts_for_a_desktop_password_without_persisting_it` | AC-7 |

## Decisions & trade-offs

- **Reuse the overlay and the bridges; never add a browser protocol client.** The daemon-bridge +
  LiveKit-track architecture is the existing design, and a second rendering path would double the
  surface for no gain.
- **Host-scoped storage separate from the session vault.** A desktop belongs to a machine; entangling
  the two would make a session's deletion remove a host's target.
- **Follow node 6's posture, not the session vault's** — prompt, encrypt, drop; do not persist a host
  desktop password. ⚠ **Deliberately inconsistent with the per-session precedent**, and worth a
  reviewer's challenge: one posture across the Hosts screen beats two, but the session vault exists and
  a reviewer may reasonably prefer it.
- **Gate on `media`, and remove rather than disable.** Consistent with `InspectorTabs`, and honest — on
  an IPC-reached host a video track genuinely cannot arrive.
- **Where the host-scoped RPCs live is left open** — `connection.proto` beside the other host RPCs, or
  a host-scoped section of `screen_sharing.proto`. Recorded as a red-phase decision rather than guessed.

## Technical debt & production readiness

- ⚠ **Real-pixel verification is manual.** The CI gate covers neither VM-backed nor desktop suites.
- ⚠ A bridge process per active host desktop; the same resource profile as the session path, now
  reachable from a screen that lists every host at once.

## Refactoring needed

_(populated by each validation phase)_

## Validation results

_(populated by each validation command)_

## TODO

- [x] Record initial discovery (`2026-09-06-desktop-connect-initial-discovery.md`)
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

None — this is the top node of `#hosts-screen`.
