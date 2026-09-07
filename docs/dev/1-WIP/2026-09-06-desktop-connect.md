# Changeset: desktop-connect

**Date:** 2026-09-06
**Status:** 🚧 In Progress
**Type:** New feature
**Stack:** `#hosts-screen` 8/8 — the top node
**Branch:** `feature/hosts-screen/desktop-connect` → base `feature/hosts-screen/desktop-probe`
**PR:** [#460](https://github.com/uppin/tddy-coder/pull/460)

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

## Prerequisites

Open items in [`docs/dev/TODO.md`](../TODO.md) this PR runs into.

### ⚠ DURING — the daemon's secret stores still truncate in place

`docs/dev/TODO.md` § *The daemon's secret stores still truncate in place* (source:
atomic-session-file-writes, 2026-08-16). `screen_sharing_vault.rs` — the file this node's
session-scoped counterpart uses — is **one of the three named**, left out of
`tddy_core::atomic_file` because `write_atomic` copies permission bits only from an existing target
and would create a swap file at the process umask on first write.

Two consequences for this node:

1. `FileHostDesktopTargetStore` is new persistence. It must not become a fourth hand-rolled writer.
   Targets are **not** credentials — label, host, port, protocol, username — so plain
   `write_atomic` is correct here, and the module should say so rather than copy the vault's pattern
   by reflex.
2. This node deliberately does **not** persist a desktop password (it follows
   `#hosts-screen 6/8`'s prompt-encrypt-drop posture), which is what keeps it out of the blocking
   case that node's private key is in. That is a reason the inconsistency with the session vault is
   defensible, not merely a preference.

### ⚠ DURING — `connection_service.rs` is 19,600 lines

`docs/dev/TODO.md` § *`connection_service.rs` is 19,600 lines*. This node adds its handlers to
`screen_sharing_service.rs` rather than to `connection_service.rs`, so it does **not** add to that
file — nor do nodes 2, 5 and 7. The four that do are **nodes 1, 3, 4 and 6**. Recorded here so the
stack's total contribution to that file is visible from any of its nodes.

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

- [x] Host-scoped target model + storage
- [x] Host-scoped start/stop RPCs + both regenerations
- [x] Bridge start/stop at host scope, reusing the existing spawn path
- [x] Connect action, gated on reachability **and** `media`
- [x] Overlay mounted at host scope — ⚠ **without input forwarding**, see below
- [x] Desktop password via node 6's encrypted prompt, not persisted — daemon raises it, browser
      answers it, nothing stores it
- [~] Rust unit/integration tests + Cypress component tests — AC-7's five are **red**

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

- [x] Target model + store, unit-tested
- [x] Host-scoped start returning overlay-ready values
- [x] Overlay mounted, a real track rendering
- [ ] Input forwarding verified — ⚠ **moved to `#hosts-screen 9/9`**, see below
- [x] Stop releasing the bridge process
- [x] Capability gating removing the action without media
- [~] Password prompt through node 6's channel, nothing persisted — daemon decrypts and drops;
      the browser does not yet prompt

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
| `opening_a_host_desktop_asks_the_operator_for_its_password` | AC-7 |
| `the_password_the_operator_typed_reaches_the_bridge_on_stdin_and_never_its_arguments` | AC-7 + AC-10 |
| `the_password_the_operator_typed_is_never_written_to_disk` | AC-7 |
| `a_desktop_password_nobody_answers_fails_the_start_rather_than_starting_without_it` | AC-7 |

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

### From @red (AC-7 red phase)

- ⚠ **`StartHostStreamRequest.encrypted_password` is now two ways to do one thing.** With the prompt
  raised by the daemon, no browser can fill that field — it has no key to encrypt under until a
  prompt arrives. Green should decide whether to delete it. If it is deleted, the existing
  `a_host_desktop_password_never_appears_in_the_bridge_process_arguments` must be rewritten to go
  through the prompt, at which point it is subsumed by
  `the_password_the_operator_typed_reaches_the_bridge_on_stdin_and_never_its_arguments`. It is kept
  for now precisely so that test is not weakened in a red phase.
- `HostPassphraseDialog`'s copy is node 6's — "is waiting to unlock … and load it into its
  ssh-agent". Reused unchanged for the desktop password, and the AC-7 spec deliberately does not
  assert on that sentence. Green should generalise it, or the operator is told a desktop password is
  going into an ssh-agent.
- `HostPromptEventLike` (`hostPromptsSubscription.ts`) carries no `kind`. Nothing needs it while a
  surface subscribes only during its own call, but two surfaces now share the feed, and a screen
  showing both at once would have no way to tell the questions apart.
- The stand-in bridge writes its recordings into the same directory the daemon stores under, so
  `files_the_daemon_wrote` has to exclude them **by name**. Installing the bridge in its own
  directory would remove the exclusion, and with it the chance of a future recording silently
  falling outside it.

## Validation results

### Red phase (draft-PR contract)

- `cargo clippy -p tddy-daemon --all-targets -- -D warnings` — clean.
- `host_desktop_targets.rs` — 5/5 red. `HostDesktopConnectAcceptance.cy.tsx` — 5/5 red.

### ⚠ Three of these tests passed *vacuously* on the first run — and were rewritten

The gating criteria are naturally phrased as absences ("no connect action when the connection carries
no media"), and `should("not.exist")` **passes trivially against unimplemented UI**: nothing renders
the action in *any* condition, so all three were green while proving nothing. They would have stayed
green through an implementation that got the gating exactly backwards.

Each is now a **contrast** test: mount the positive case and assert the action exists, then mount the
negative case and assert it is withdrawn. A component that never renders the action fails the first
half; one that always renders it fails the second. Only correct gating satisfies both.

Same failure mode as a stubbed `pending_prompt_pump_count()` returning `0` in `#hosts-screen 6/8` —
an assertion that holds for the wrong reason is worse than no assertion, because it reports the
property as verified forever.

### Green phase

Implemented on a rebase onto `desktop-probe`'s current tip (node 7 had been greened and
force-pushed; this branch had been carrying pre-rewrite copies of nodes 1-7's commits).

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy -p tddy-daemon --all-targets -- -D warnings` | clean |
| `cargo test -p tddy-daemon --lib` | **754 passed, 0 failed** |
| `cargo test -p tddy-daemon --test screen_sharing_service_acceptance` | **6 passed** (AC-9, the reused session path) |
| `HostDesktopConnectAcceptance.cy.tsx` | **5 passed** |
| `HostsScreenRemoteDesktopAcceptance.cy.tsx` (node 7 regression) | **4 passed** |

The four integration tests named in *Acceptance tests* above **did not exist** — the red phase
wrote none for `screen_sharing_service.rs`, which had no test module at all, and left all five
host-scoped RPCs as `unimplemented!()`. They were written during green, under the names planned
here, and each was **mutation-checked** rather than trusted on a first-run green — the same
vacuous-pass failure this document already records for three Cypress tests. One of them
(`starting_a_session_desktop_still_works_unchanged`) passed its first mutation because it restated
the production function; it now asserts the literal session key shape.

`AC-10` is guarded against the same vacuity: the daemon passes *no* argv at all, so "the password
is not in argv" is trivially true. The test first asserts the password **is** in the JSON the
bridge read from stdin, so a password that never travelled fails before the argv assertion.

### ⚠ The media-gating spec asserted a condition it never constructed

`offers connect over a media carrying connection but not over one without media` built its negative
half by changing the **daemon directory list**. That has nothing to do with the connection registry:
`withSelectedDaemon` always injects a `Room`, and `LiveKitConnectionProvider.connectHost`
(`liveKit.tsx:258`) claims every host once a room exists, with all three capabilities. Both halves
therefore mounted an identical media-capable connection, and the test passed against a component
that never rendered the action at all — the third instance of this failure mode in this node.

Both halves now mount over a **stated** wire (`aHostConnection(HOST)` + `room={null}`), the pattern
`cypress/support/rpc/hostConnections.ts` documents for precisely this and that
`PresenceCapabilityGatingAcceptance.cy.tsx` already uses. The contrast is now within one mount shape
differing only in capabilities, so neither a never-render nor an always-render component passes.

### ⚠ Input forwarding does not exist to reuse — AC-3 is blocked on a scope decision

`## Responsibility` says this node mounts the existing overlay "with input forwarding", and
`## Boundaries` says it does **not** change `ScreenSharingOverlay` or `vncInput.ts` because it
"reuses them". Both cannot hold:

- `ScreenSharingOverlay.tsx`'s only key/mouse handlers are **Escape-to-close** and
  **click-outside-to-close** (lines 65-85). It forwards nothing to the remote desktop.
- `vncInput.ts` is referenced by **nothing in production** — only by its own `vncInput.test.ts`.
  It is orphaned from a `VncOverlay` that no longer exists.

So AC-3 is new plumbing in a file this node's boundaries forbid it to touch, not a reuse. Left
unimplemented and marked, rather than silently widening the boundary or quietly dropping the AC.
Marked `TODO(#hosts-screen 8/8)` at `HostDesktopOverlay.tsx:132`.

### ⚠ AC-7's browser half is unimplemented — its planned spec was never written

*Acceptance tests* names `prompts_for_a_desktop_password_without_persisting_it`; that spec is absent
from `HostDesktopConnectAcceptance.cy.tsx`. The **daemon** half is complete and tested —
`StartHostStream` decrypts `encrypted_password` with node 6's `HostKeypair::decrypt`, uses it, drops
it, persists nothing. The **browser** half — prompting over node 6's channel and encrypting the
answer — is not implemented, and no test covers it. Marked `TODO(#hosts-screen 8/8)` at
`HostDesktopOverlay.tsx:101`.

### Red phase — AC-7, the daemon-raised desktop-password prompt

The green wave above left AC-7's browser half unimplemented and its planned Cypress spec unwritten
(see the section above it). This wave writes the failing tests for the **whole** of AC-7, daemon and
browser, against a shape the earlier wave did not have: the prompt is **raised by the daemon**, not
supplied by the caller.

**Why the design changed.** `StartHostStream.encrypted_password` assumed the browser already held
this host's public key. It cannot: `HostPromptEvent.host_public_key` (`connection.proto`) is the only
place that key is published, and it also carries the fingerprint the client pins. A browser that has
not been asked anything therefore has nothing to encrypt under, which is why the earlier wave's
browser half could not be written at all.

**Surface added** (additive; node 6's crypto, keypair and pinning rules untouched):

| Change | Why the tests need it |
|---|---|
| `HOST_PROMPT_KIND_DESKTOP_PASSWORD = 2` on `HostPromptKind` (`connection.proto`) | the kind the tests assert on, in both languages |
| `PromptKind::DesktopPassword` + its `wire_kind` arm | the Rust half of the same |
| `HostScope.prompts: Arc<dyn HostPromptRegistry>`, third argument to `with_host_scope` | the daemon has to have a channel to raise the question on. `#[allow(dead_code)]` + TODO until green reads it |
| `runtime.rs` builds **one** registry and hands it to both `ConnectionService` and `ScreenSharingService` | a prompt raised on one registry and answered on another is a question nobody can answer |
| `hostPromptFeed.ts` frames may state their `kind` (defaults to the ssh passphrase) | node 6's specs are unaffected; node 8's says which question it means |

**Failing tests.** Four in `screen_sharing_service.rs`, one in
`HostDesktopConnectAcceptance.cy.tsx`, all red for missing behaviour rather than for a compile or a
fixture error.

**Guards against a vacuous pass** — the failure mode this document already records four times:

- *"the password is not in argv"* is trivially true (the daemon passes **no** argv). Both argv and
  disk assertions are therefore preceded by `config["password"] == A_DESKTOP_PASSWORD` read from the
  bridge's **stdin**: a password that never travelled fails there first.
- The disk scan was proven, with a throwaway test, to cover the target store, to walk nested
  directories, to find a password planted under them, and to exclude the stand-in bridge's own
  recording (which is a test asking the bridge to write down what it was handed, not the daemon
  writing a secret down).
- The operator double was proven to deliver a **decryptable** answer under this host's real key, so
  green is implementing against a working channel rather than a broken fixture.
- `everythingSentTo` in the Cypress spec was proven, with a throwaway spec, to catch a plaintext
  password on `AnswerHostPrompt` **and** on `StartHostStream.encryptedPassword`; the storage scan was
  proven to catch one kept in either browser store.
- The Cypress spec asserts a **positive** fact first (exactly one answer reached the host that
  asked), so every absence assertion after it runs in a world where the password really existed.

**Wired, not asserted.** `a_daemon()`'s operator answers every question with the *empty* password,
because every desktop these tests attach is password-less. Stated rather than left absent so that
the existing tests about rooms, identities and stop keep passing once green starts prompting,
instead of turning into prompt tests or hanging.

### Open decisions a reviewer should weigh

- **Room:** a host desktop publishes into the daemon's configured `livekit.common_room` — a session
  has a room in its metadata, a host has none, and the common room is the one a browser already
  holds a token for on this screen. A daemon with none configured gets `FAILED_PRECONDITION`.
  Deliberately not gated on `livekit.enabled`, which governs whether *this daemon* joins.
- **Bridge identity** is `screenshare-host-{instance_id}-{target_id}`; the host id must be present
  because every host's bridge lands in the same common room. Track name is unchanged.
- **`add_host_target` stores the protocol as sent**, mirroring the session path's
  `unwrap_or(Unspecified)` posture rather than rejecting an unknown value; the spawn path logs and
  skips an unusable target. Pattern-consistency was chosen over an untested validation branch.


### Green phase — AC-7, the daemon-raised desktop-password prompt

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy -p tddy-daemon --all-targets -- -D warnings` | clean |
| `cargo test -p tddy-daemon --lib` | **757 passed, 0 failed** |
| `cargo test -p tddy-daemon --test screen_sharing_service_acceptance` | 6 passed |
| `HostDesktopConnectAcceptance.cy.tsx` | **8 passed** |
| `HostAddKeyAcceptance` · `HostsScreenAddKeyAcceptance` · `HostsScreenRemoteDesktopAcceptance` | 10 · 6 · 4 — nodes 6 and 7 unregressed |

`StartHostStream` issues a prompt, waits exactly as long as it is answerable, decrypts with node 6's
`HostKeypair::decrypt`, hands the plaintext to the bridge on stdin and drops it. An unanswered prompt
fails with `DeadlineExceeded` and spawns nothing.

**`StartHostStreamRequest.encrypted_password` was removed**, resolving the red phase's open question.
With the prompt daemon-raised no browser can fill it — the host's public key is published only on
`HostPromptEvent`, so before a prompt arrives there is nothing to encrypt under and no fingerprint to
pin. Its test asserted the property through that dead path and is **superseded** by
`the_password_the_operator_typed_reaches_the_bridge_on_stdin_and_never_its_arguments`, which asserts
the same thing through the real flow, stdin-first so the argv half cannot pass vacuously.

`answer_before_expiry` moved into `host_prompts.rs` beside the registry: two callers now need "wait
exactly as long as this prompt is answerable", and two copies are two chances for one to drift.

### ⚠ A password-less desktop was locked out — caught after green, fixed

The daemon asks on **every** start, and `HostPassphraseDialog` disabled submit on an empty answer
(`blocked = keyChanged || encrypting || passphrase.length === 0`). A desktop with no password was
therefore unopenable from the browser at all — a live break of **AC-2**, not a cosmetic gap, and it
would have shipped marked delivered.

Fixed with an additive `allowEmpty`, defaulting to `false` so node 6's ssh-key question is byte-for-byte
unchanged. Pinned **from both sides**, because one assertion could not catch both mistakes: a dialog
that never accepts empty fails `opens a desktop that has no password from an empty answer`; one that
always accepts empty fails the new `refuses to send an empty passphrase, which unlocks no key` in
node 6's own spec. Both mutations were run, not reasoned about.

### Node 6-owned files touched — for the reviewer

Additive or behaviour-preserving, and called out because a node 6 reviewer will see their files here:
an optional `wording` prop on `HostPassphraseDialog` (defaults to node 6's exact sentences — without
it an operator is told their desktop password is going into an ssh-agent), the optional `allowEmpty`
above, an optional `kind?` on `HostPromptEventLike` (two surfaces now share one feed), the additive
`HOST_PROMPT_KIND_DESKTOP_PASSWORD` arm, and the `answer_before_expiry` move. **The crypto, the
keypair lifecycle, the pinning rules and the channel's behaviour are unchanged**, and node 6's specs
are green.

⚠ **Known, not fixed here:** `HostAddKeyAction` does not filter the feed by kind, so with an add-key
and a desktop-open in flight on the same host at once its dialog could pick up the desktop prompt.
This node's surface filters; fixing the other direction is node 6's.

### AC-3 moved to `#hosts-screen 9/9`

Investigation established that input forwarding is **not** a reuse and never was — both ends exist and
the entire middle is missing:

| Layer | State |
|---|---|
| Browser translate (`vncInput.ts` — scaling, X11 keysyms) | exists, tested |
| Browser capture and send | missing |
| Wire (`VncInputService.StreamInput`) | proto defined, **implemented by neither end** |
| Daemon: serve it, route to a bridge | missing |
| Daemon → bridge channel | **none exists** — the bridge reads one JSON config from stdin and never reads again |
| Bridge → VNC/RDP server | `vnc_client.rs` has `pointer_event`/`KeyEvent`, `tddy-rdp` has `MousePdu` — **nothing calls them** |

Delivering it means changing `packages/tddy-vnc`, `packages/tddy-rdp` and the bridge spawn path — all
three named in `## Boundaries` — so it becomes its own node rather than widening this one. That node
also repairs the **per-session** path, which has never forwarded input either despite `VncOverlay`'s
docblock claiming it "Captures pointer and keyboard events". Its open design question is what the
daemon→bridge channel should be: a second pipe, a unix socket, or a LiveKit data channel.

⚠ Correcting this document's earlier claim that `vncInput.ts` was "orphaned from a `VncOverlay` that
no longer exists": `VncOverlay.tsx` **does** exist. It simply never forwarded input either.


## TODO

- [x] Record initial discovery (`2026-09-06-desktop-connect-initial-discovery.md`)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [~] TDD Red (AC-7) — failing tests for the daemon-raised desktop-password prompt
- [ ] TDD Green (AC-7) — raise the prompt, await the answer, prompt in the browser
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
