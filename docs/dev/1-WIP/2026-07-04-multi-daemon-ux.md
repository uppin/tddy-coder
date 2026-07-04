# Changeset: multi-daemon-ux — host selection, reliable add-to-host, PR-stack modal, auto-clone

**Date:** 2026-07-04  
**Branch:** `multi-daemon-ux`  
**Packages:** `tddy-web`, `tddy-daemon`  
**Feature PRDs:**
- [docs/ft/web/daemon-selector-livekit-rpc.md](../../ft/web/daemon-selector-livekit-rpc.md)
- [docs/ft/web/projects-screen-multi-host.md](../../ft/web/projects-screen-multi-host.md)
- [docs/ft/web/session-drawer.md § PR-Stack Chat Screen](../../ft/web/session-drawer.md)

## Problem

Multi-daemon flows were incomplete or broken: session creation always ran on the selected
daemon (`daemonInstanceId:""`), "Add to host" sent its RPC to the currently-selected daemon
instead of the chosen target, PR-stack "Start session" fired a hardcoded `startSession` with no
review step, a session started on a host lacking the project failed hard, and each host's base
clone location was invisible in the UI.

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] R1: Host `<select>` in `CreateSessionPane` (`useDaemons()`, rendered only when daemons exist), `daemonInstanceId` threaded into `startSession` + `listProjectBranches`
- [x] R3: `CreateSessionInitialValues` prefill prop on `CreateSessionPane`; new `CreateSessionDialog` overlay; PR-stack "Start session" opens the pre-filled modal
- [x] R2: `useDaemonClientFor(service, instanceId)`; add-to-host sends over a client built for the chosen target host (transport factory + `daemonRpcIdentity`)
- [x] R5: optional clone-location input in add-to-host → `AddProjectToHostRequest.user_relative_path`
- [x] R4 (web): `parseDaemonAdvertisement` reads `repos_base_path` → `DaemonHost.reposBasePath`; Projects shows each host's base location
- [x] R4 (daemon): `project_provision::ensure_project_available_locally`; wired into `start_session` (all four start paths); `repos_base_path` published in the common-room advertisement

## Acceptance tests

- [x] `packages/tddy-web/cypress/component/CreateSessionHostSelectionAcceptance.cy.tsx` (R1)
- [x] `packages/tddy-web/cypress/component/PrStackStartSessionModalAcceptance.cy.tsx` (R3; replaces the deleted `PrStackStartSessionAcceptance.cy.tsx`)
- [x] `packages/tddy-web/cypress/component/ProjectsScreenAcceptance.cy.tsx` (R2/R5/R4-web, +3)

## Unit tests

- [x] `packages/tddy-web/src/lib/participantRole.test.ts` — `reposBasePath` parse (+2)
- [x] `packages/tddy-daemon/src/project_provision.rs` — `ensure_project_available_locally` (4)
- [x] `packages/tddy-daemon/src/livekit_peer_discovery.rs` — advertisement `repos_base_path` round-trip

## Validation Results

### /pr-wrap validation (2026-07-04)

**Blockers:** none.

**Should-fix (1, fixed):**
- `prstack/PrStackScreen.tsx` `handleStartSession` dropped the null-client guard in the refactor —
  with no daemon connected, the row entered its "starting" state but the (client-gated) dialog
  never opened, leaving it stuck. **Fixed:** restored `if (!client) return;`.

**Nits (1 fixed, 1 accepted):**
- `CreateSessionPane.tsx` host `<select>` could display a daemon while state stayed `""` when the
  pre-filled host was empty. **Fixed:** empty pre-filled host now falls through to the selected
  daemon (`||`), aligning displayed option with submitted value.
- `ProjectsAppPage.tsx` builds a fresh add-to-host client per submit (not memoized) — harmless for
  a click handler; left as-is.

**Clean (verified):**
- Risks/correctness — `useDaemonClient`→`useDaemonClientFor` refactor preserves null-handling;
  `clientForHost` guards null room; list/create still use the selected daemon. Rust
  `ensure_project_available_locally` surfaces clone failures (no masking), preserves `NotFound`
  (not flattened to `internal`), runs the clone off the async executor; advertisement field is
  backward-compatible (`#[serde(default)]`).
- Prod-readiness — no phase-marker comments, no new debug prints/TODO/FIXME, no test-only branches.
- Test quality — new specs are fluent-style, deterministic, page-object-driven.

**Suite status:**
- `tddy-daemon`: 230 pass / 1 fail (pre-existing env: `tddy-sandbox-runner` not built; unrelated).
  Canonical `cargo clippy -- -D warnings` + `cargo fmt` clean.
- Web: `CreateSessionHostSelectionAcceptance` 3/3, `PrStackStartSessionModalAcceptance` 3/3,
  `CreateSessionPane` 28/28 (regression), `ProjectsScreenAcceptance` 7/7, `participantRole` 7/7.

> Doc-wrapping (moving this changeset out of `docs/dev/1-WIP/`) is deferred to merge time.

## Delta summary

### `tddy-web`

**New files:**
- `src/components/sessions/CreateSessionDialog.tsx` — overlay wrapper (CodexOAuthDialog pattern:
  fixed inset, `role="dialog"`/`aria-modal`, `open`/`onClose`, `max-h-[90vh]` scroll, testid
  `create-session-dialog`) rendering `CreateSessionPane`.

**Modified files:**
- `src/components/sessions/CreateSessionPane.tsx` — Host `<select>` (`create-session-host-select`)
  from `useDaemons()`, shown only when daemons exist; `daemonInstanceId` state
  (`initialValues?.daemonInstanceId ?? selectedInstanceId ?? ""`) threaded into `startSession`
  and `listProjectBranches`; optional `initialValues` (`CreateSessionInitialValues`) prefill prop.
- `src/components/sessions/prstack/PrStackScreen.tsx` — "Start session" opens the pre-filled
  `CreateSessionDialog` (branch, prompt = title+description, stack parent, project, host from the
  orchestrator session) instead of calling `startSession` directly; fires `onChildSessionStarted`
  on success.
- `src/rpc/selectedDaemon.tsx` — added `useDaemonClientFor(service, instanceId)`; `useDaemonClient`
  now delegates to it.
- `src/components/projects/ProjectsAppPage.tsx` — add-to-host sends over a client built for the
  chosen target host (room + `useLiveKitTransportFactory` + `daemonRpcIdentity`); list/create still
  use the selected-daemon client.
- `src/components/projects/ProjectsScreen.tsx` — optional clone-location input
  (`project-add-to-host-user-relative-path-<id>`) threaded to `userRelativePath`; per-host base
  location (`project-host-base-location-<id>`) from `DaemonHost.reposBasePath`.
- `src/lib/participantRole.ts` — `DaemonHost.reposBasePath?`; `parseDaemonAdvertisement` reads
  `repos_base_path`; `daemonHostsFromParticipants` carries it through.

### `tddy-daemon`

**New files:**
- `src/project_provision.rs` — `ensure_project_available_locally(projects_dir, project_id,
  repos_base_dir, cloner, peer_lookup)`: returns a registered+on-disk project as-is; clones from
  the stored `git_url` when registered-but-missing; else peer-discovers `(name, git_url)`, clones
  into `repos_base_dir/<name>` and registers via `add_or_get_project`; `NotFound` when unknown
  everywhere. Clone failures surface as errors.

**Modified files:**
- `src/connection_service.rs` — `ensure_project_available_for_start` runs the helper via
  `spawn_blocking` + `timeout` (injecting a real cloner — `SpawnClient::clone_repo` when a spawn
  worker is configured, else `spawner::clone_as_user` — and a peer-lookup from
  `eligible_daemon_source`), called on the local branch of `start_session` so all four start paths
  auto-provision uniformly.
- `src/livekit_peer_discovery.rs` — `repos_base_path` added to `DaemonAdvertisement` (+ wire
  field), published from `config.repos_base_path_or_default()` and parsed back
  (`parse_daemon_advertisement_json`). JSON key `repos_base_path` matches the web parser.
- `src/lib.rs` — `pub mod project_provision;`.
- `tests/*` — `DaemonAdvertisement` literals updated for the new field.
