# Changeset: Native notifications in Tddy Desktop

**Created:** 2026-10-25
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — two Rust packages plus Tauri config, a new external dependency (`tauri-plugin-notification`), an embedding API on the daemon runtime, platform-dependent click delivery (unverified)
**Stack:** `#home-inbox 4/8` — branch `feature/home-inbox/desktop-notify`, base `feature/home-inbox/elicit-content`
**PRD:** [PRD-2026-10-25-desktop-notifications.md](../../ft/desktop/1-WIP/PRD-2026-10-25-desktop-notifications.md)

## Initial Discovery

[2026-10-25-home-inbox-desktop-notify-initial-discovery.md](./2026-10-25-home-inbox-desktop-notify-initial-discovery.md)

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-09-05-from-2026-09-05-tauri-desktop-single-process-daemon.md](../todo/2026-09-05-from-2026-09-05-tauri-desktop-single-process-daemon.md) | ⚠ During — Linux is unverified for the app as a whole; this node's native path is verified on macOS and recorded as unverified on Linux. The Telegram lifecycle-message TODO in `lib.rs` is unrelated |
| [2026-09-18-desktop-install-configures-no-identity.md](../todo/2026-09-18-desktop-install-configures-no-identity.md) | — Unrelated (no sessions without identity ⇒ no notifications; nothing to change here) |
| [2026-09-20-tauri-dev-restarts-the-app-for-generated-test-fixtures.md](../todo/2026-09-20-tauri-dev-restarts-the-app-for-generated-test-fixtures.md) | ⚠ During — tests must not write fixtures under the watched `src-tauri` tree |

## Affected Packages

- [ ] [`tddy-daemon`](../../../packages/tddy-daemon/README.md): `RuntimeOptions::with_notification_subscriber`, appended in `mount_host`
- [ ] [`tddy-desktop`](../../../packages/tddy-desktop/README.md): `native_notifications` subscriber, plugin, capability, click → navigate

## Responsibility

- `tddy_daemon::runtime::RuntimeOptions::with_notification_subscriber(Arc<dyn SessionNotificationSubscriber>)`; `mount_host` appends every registered subscriber after the two built-ins.
- `tddy_desktop::native_notifications::{DesktopNotificationSubscriber, NativeNotifier (port), DesktopNotice { title, subtitle, body, sound, route }, desktop_notice_for(&SessionNotification, &DesktopContext) -> Option<DesktopNotice>}`
  — `AttentionRequired` only; owner = the desktop operator's OS user; agent title or "`<label>` needs you"; body via `1/8`'s `to_plain_text(md, 300)` or the fixed `text`; sound when the agent asked for it or the source is `PendingQuestion`; suppression when the focused window shows that session or the notice equals the previous one for that session.
- The Tauri `NativeNotifier` over `tauri-plugin-notification`; permission requested on first use, refusal logged once.
- Click: focus/unminimise `MAIN_WINDOW_LABEL` and `navigate` to `#/sessions/<id>?host=<instance>`.
- `Cargo.toml` dependency, `capabilities/default.json` notification permission.

## Boundaries

- No change to the bus's built-in subscribers, Telegram or the web.
- No notification actions, no peer daemons' sessions, no Windows.
- The binary `tddy-daemon` registers nothing new.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `#home-inbox 1/8` agent-notify | `SessionNotification.agent` (`AgentNotification`), `notification_markdown::to_plain_text` | builds the notice title/body from them | change the model or the renderer |

**Base (replan 2026-10-26).** Nothing from `#start-parity` is consumed. `mount_host.rs` gained
`#coagents`' launcher and approval routes on `master`; the subscriber registration goes beside
them once the `#start-parity` line is rebased onto `master`.

`#home-inbox 3/8` sits below but is not consumed: its pending-question notifications already carry a
human `text`, which this node shows like any other attention notification, and its `PendingQuestion`
source only selects the sound.

## Draft PR contract

The first push publishes `RuntimeOptions::with_notification_subscriber`, the subscriber and
`NativeNotifier` port signatures, `desktop_notice_for` (`// TODO(desktop-notify): implement`), the
dependency + capability, and the failing tests.

## Green wave

**Wave:** 2 of 4
**Greenable independently:** no — `desktop_notice_for`'s tests build bodies with `1/8`'s
`to_plain_text` (behaviour).
**Concurrent with:** `#home-inbox 3/8` elicit-content, `5/8` terminal-keys
**Blocks:** nothing

    n1 → n3, n4, n7      n2 → n5      n3 → n6, n7      n6 → n8      (cross-stack, replan 2026-10-26: #start-parity #750 [merged] → n2, n5, n6, n7, n8 · #755 → n3 · #751, #758 → n6 · #coagents [merged] → n6)

## State A

The bus's subscribers are fixed in `tddy-daemon/src/runtime/mount_host.rs`; `DaemonRuntime` exposes
no bus. Tddy Desktop has `tauri-plugin-opener` only, no notification/deep-link/focus code; the only
navigation precedent is `window.navigate` in the OAuth callback.

## State B

`start_daemon` builds the runtime with the desktop subscriber; an attention notification for the
operator's session becomes one native notification; clicking it focuses the window and opens the
session route.

## Delta

### New
- `native_notifications.rs` (tddy-desktop); dependency `tauri-plugin-notification`
### Modified
- `RuntimeOptions` (+subscriber list), `mount_host::open_shared`
- `tddy-desktop/src-tauri/{src/lib.rs (start_daemon, plugin init), Cargo.toml, capabilities/default.json}`
- `docs/ft/desktop/tddy-desktop-tauri.md` at wrap

## Testing Strategy

### Acceptance tests

**`tddy-desktop`**: `packages/tddy-desktop/src-tauri/tests/native_notifications_acceptance.rs` (recording `NativeNotifier`)
- [ ] `an_agent_notification_shows_one_native_notification_with_its_title_label_and_plain_text_body`
- [ ] `finished_and_needs_input_show_their_fixed_text_and_activity_shows_nothing`
- [ ] `a_notification_for_the_session_the_focused_window_shows_is_not_repeated_natively`
- [ ] `another_os_users_session_is_never_shown`
- [ ] `a_click_navigates_the_main_window_to_the_sessions_route` (route builder + click handler over a fake window)

**`tddy-daemon`**: `packages/tddy-daemon/tests/runtime_notification_subscriber_acceptance.rs`
- [ ] `a_subscriber_registered_on_runtime_options_receives_the_runtimes_notifications`

### Manual verification (recorded in Validation Results)
- [ ] macOS: a real notification appears and a click opens the session (plugin click delivery — **risk**; if the platform cannot deliver clicks, stop and ask for the fallback).

### Test level decisions
| Aspect | Level | Rationale |
|---|---|---|
| Notice content/suppression | Integration over the subscriber with a recording port | OS notification centres are not testable in CI |
| Embedding hook | Integration over `runtime::build` | The seam is the API |

## Decisions & Trade-offs

- `tauri-plugin-notification` (developer, 2026-10-25); `RuntimeOptions` hook rather than an in-process stream (developer, 2026-10-25).
- Body cut to 300 plain-text characters — operating-system centres truncate further anyway.

## TODO

- [x] Record initial discovery (`2026-10-25-home-inbox-desktop-notify-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
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
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap

## Refactoring Needed
### From /red (TDD red phase)
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |
