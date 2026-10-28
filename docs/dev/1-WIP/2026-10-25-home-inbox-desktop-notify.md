# Changeset: Native notifications in Tddy Desktop

**Created:** 2026-10-25
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — two Rust packages plus Tauri config, a new external dependency (`tauri-plugin-notification`), an embedding API on the daemon runtime, platform-dependent click delivery (unverified)
**Stack:** `#home-inbox 4/8` (PR #769) — branch `feature/home-inbox/desktop-notify`, base `feature/home-inbox/elicit-content`
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
- [ ] [`tddy-desktop`](../../../packages/tddy-desktop/README.md): `native_notifications` subscriber, plugin, click handler
- [ ] `flake.nix` (dev shell, macOS only): an Objective-C-only C compiler wrapper, so `mac-notification-sys` builds

## Responsibility

- `tddy_daemon::runtime::RuntimeOptions::with_notification_subscriber(Arc<dyn SessionNotificationSubscriber>)`; `mount_host` appends every registered subscriber after the two built-ins.
- `tddy_desktop::native_notifications::{DesktopNotificationSubscriber, NativeNotifier (port), DesktopNotice { title, subtitle, body, sound, route }, desktop_notice_for(&SessionNotification, &DesktopContext) -> Option<DesktopNotice>}`
  — `AttentionRequired` only; owner = the desktop operator's OS user; agent title or "`<label>` needs you"; body via `1/8`'s `to_plain_text(md, 300)` or the fixed `text`; sound when the agent asked for it or the source is `PendingQuestion`; suppression when the focused window shows that session or the notice equals the previous one for that session.
- The Tauri `NativeNotifier` (`TauriNotifier`) over `tauri-plugin-notification` 2.4. The plugin's desktop
  notification has no subtitle, so the notifier shows `DesktopNotice::body_with_label()` — the label as
  the first line, then the body. It requests no permission and logs no refusal: the plugin answers
  "granted" without asking on desktop, and macOS silently drops a refused notification.
- `DashboardWindow` port (`TauriDashboardWindow`): focus, current location, bring to front, navigate.
  `SessionRoute { session_id, host }` with `fragment()` and `url_within(&Url)`.
- Click handler `open_session_route`: bring `MAIN_WINDOW_LABEL` to the front, then `navigate` to
  `#/sessions/<id>?host=<instance>`. **Not wired to a click** — plugin 2.4 reports none; see
  `docs/dev/todo/2026-10-26-desktop-notification-click-needs-tauri-2-12.md`.
- `Cargo.toml` dependencies (`tauri-plugin-notification ~2.4`, `async-trait`, `tddy-daemon-kernel`). No
  capability: only Rust calls the plugin.
- `flake.nix`: on macOS the dev shell exports `CC_aarch64_apple_darwin` / `CC_x86_64_apple_darwin` as a
  wrapper that adds `-include <SDK>/usr/include/uuid/uuid.h` for `.m`/`.mm` sources only.

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

**Sequencing (expected).** `an_agent_notification_shows_one_native_notification_with_its_title_label_and_plain_text_body`
and the unit test `an_agent_body_is_cut_to_the_body_limit_and_ends_in_an_ellipsis` need `1/8`'s real
`to_plain_text`, which is a stub on this branch. They cannot go green until `#766` is rebased in.

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
- `tddy-desktop/src-tauri/{src/lib.rs (start_daemon, plugin init), Cargo.toml}`
- `flake.nix` (macOS dev shell)
- `docs/ft/desktop/tddy-desktop-tauri.md` at wrap

## Testing Strategy

### Acceptance tests

**`tddy-desktop`**: `packages/tddy-desktop/src-tauri/tests/native_notifications_acceptance.rs` (recording `NativeNotifier`; shared fakes in `tests/support/mod.rs`)
- [ ] `an_agent_notification_shows_one_native_notification_with_its_title_label_and_plain_text_body`
- [ ] `finished_and_needs_input_show_their_fixed_text_and_activity_shows_nothing`
- [ ] `a_notification_for_the_session_the_focused_window_shows_is_not_repeated_natively`
- [ ] `another_os_users_session_is_never_shown`
- [ ] `a_click_navigates_the_main_window_to_the_sessions_route` (route builder + click handler over a fake window)

**Unit** (`tests/native_notifications_unit.rs`, 18 tests): `desktop_notice_for` (title, 300-char body limit, sound rules, route), `SessionRoute::fragment` / `url_within`, `DesktopNotice::body_with_label`, subscriber suppression (focus, same-as-previous, per session).

**`tddy-daemon`**: `packages/tddy-daemon/tests/runtime_notification_subscriber_acceptance.rs`
- [ ] `a_subscriber_registered_on_runtime_options_receives_the_runtimes_notifications`

### Manual verification (recorded in Validation Results)
- [ ] macOS: a real notification appears (label first line, then body). Click delivery is out of scope — see the TODO file.

### Test level decisions
| Aspect | Level | Rationale |
|---|---|---|
| Notice content/suppression | Integration over the subscriber with a recording port | OS notification centres are not testable in CI |
| Embedding hook | Integration over `runtime::build` | The seam is the API |

## Decisions & Trade-offs

- `tauri-plugin-notification` (developer, 2026-10-25); `RuntimeOptions` hook rather than an in-process stream (developer, 2026-10-25).
- **Click delivery (orchestrator, developer AFK, 2026-10-26).** No Tauri bump: keep `tauri-plugin-notification ~2.4` (2.5+ needs Tauri 2.12 and Rust 1.90), ship without a click callback, keep `open_session_route` and its test. Wiring it is `docs/dev/todo/2026-10-26-desktop-notification-click-needs-tauri-2-12.md`; no deferral consent is recorded — it goes to the developer at the TODO gate.
- **Nix macOS build (orchestrator, developer AFK, 2026-10-26).** `mac-notification-sys` compiles Objective-C that imports Cocoa, and the nix SDK 14.4 headers lack `uuid_string_t`. Fixed in `flake.nix`, darwin only: a wrapper compiler for `.m`/`.mm` sources (nix clang, never Xcode's), exported per target; a global `CFLAGS` was rejected because it breaks `ring`'s `.S` files. Verified with no manual env: `cargo check -p tddy-desktop --all-targets`, `ring` rebuilt, `libsqlite3-sys` rebuilt.
- **Dependencies (orchestrator, developer AFK, 2026-10-26).** `async-trait` and `tddy-daemon-kernel` accepted (already in the workspace).
- **Capability (orchestrator, developer AFK, 2026-10-26).** No `notification:default` permission: only Rust calls the plugin, so the page is granted nothing.
- **Subtitle (orchestrator, developer AFK, 2026-10-26).** `DesktopNotice.subtitle` stays; `TauriNotifier` folds the label into the body (`body_with_label`) because plugin 2.4 drops the subtitle on desktop.
- **Permission (orchestrator, developer AFK, 2026-10-26).** "Request on first use, log a refusal once" dropped: plugin 2.4 cannot observe it. macOS silent-drop recorded in the PRD.
- **Contract deviations from this document's first draft.** A `DashboardWindow` port and a `SessionRoute` type were added (suppression and click both need the window, which does not exist when the runtime is built); `DesktopNotice.route` is a `SessionRoute`. Green's `mount_host.rs` append needs a small forwarding adapter, because `SessionNotificationBus::with_subscriber` is generic over a sized subscriber and cannot take `Arc<dyn …>`.
- **Acceptance-test review (orchestrator, developer AFK, 2026-10-26)** approved the six acceptance tests.
- Body cut to 300 plain-text characters — operating-system centres truncate further anyway.
- **Green (orchestrator, developer AFK, 2026-10-28).** The fixed text of a daemon-worded notification is shown whole, not cut to 300: the changeset's rule names the fixed text as the body, and the daemon's fixed sentences are short. A pending question's text is the one that can run long; a cut there is a follow-up if it shows in practice.
- **Green (orchestrator, developer AFK, 2026-10-28).** Suppression: the window must have focus *and* its page must be on the session's exact route (`SessionRoute::fragment`). A notice is recorded as "last shown" only when it is actually shown, so a notice the focused window swallowed is not treated as the previous one.
- **Green (orchestrator, developer AFK, 2026-10-28).** `wants` and `deliver` share one rule, `desktop_notice_for`, so the subscriber's filter and its notice cannot disagree. A poisoned suppression map is recovered rather than dropping the subscriber.
- **Green (orchestrator, developer AFK, 2026-10-28).** The route's host is `tddy_daemon_kernel::daemon_identity::local_instance_id_for_config` of the desktop's config, the id the daemon itself answers to. Route fragments are built and parsed by `url` (`path_segments_mut`, `query_pairs_mut`), so encoding matches the test's `dev+box%261`.
- **Green (orchestrator, developer AFK, 2026-10-28).** The desktop's OS user is `username_for_uid(getuid())`, the same resolution the daemon uses for enrolment. An unresolvable user fails the desktop's start rather than showing notifications for everyone or for no one. Tddy Desktop is unix-only, so the non-unix arm is a bail, not a fallback.
- **Green (orchestrator, developer AFK, 2026-10-28).** `start_daemon` registers the subscriber and the notification plugin is initialised in `run()`. Both are in this changeset's Delta and the PRD's State B; no TODO marker names them, so they were not in the stub list.
- **Green (orchestrator, developer AFK, 2026-10-28).** The default sound is the operating system's `default` alert, set only when `DesktopNotice.sound` is true. On Linux the freedesktop theme may not carry that name; the sound is unverified there.
- **Green (orchestrator, developer AFK, 2026-10-28), finding.** `tauri-plugin-notification` 2.4 shows a desktop notification on a task it spawns and drops that task's error, so a failure inside the OS centre is not reported to the subscriber. Only a failure before the task starts reaches `show`'s caller. Recorded in `TauriNotifier`'s doc.
- **Green (orchestrator, developer AFK, 2026-10-28), open question for the developer.** A pending question can produce a second attention notification for the same prompt: a status report that follows an already-recorded question is published with the question attached and different text, so the identical-text rule does not catch it. The kernel's `QuestionNotice.request_id` could. Not implemented: it is a third suppression rule the approved acceptance tests do not cover.
- **Green (orchestrator, developer AFK, 2026-10-28).** `tddy-daemon/tests/test_placement.rs` keeps an allowlist of suites that belong in that package. The contract's `runtime_notification_subscriber_acceptance.rs` was not on it, so the tripwire failed. The suite mounts `runtime::build` on the daemon, so it is registered (the array goes from 31 to 32), not moved.

## TODO

- [x] Record initial discovery (`2026-10-25-home-inbox-desktop-notify-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (orchestrator, developer AFK, 2026-10-26)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code (`/green`, 2026-10-28)
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
