# 2026-10-28 — Native notifications in Tddy Desktop

**Type:** Feature

`#home-inbox` 4/8 ([#769](https://github.com/uppin/tddy-coder/pull/769)). Tddy Desktop subscribes
to its embedded daemon's notification bus and shows attention notifications natively. Across:

- `tddy-daemon` — `RuntimeOptions.notification_subscribers` and `with_notification_subscriber`;
  `mount_host::open_shared` appends them after the built-in subscribers through a forwarding
  `RegisteredSubscriber` (the bus takes sized subscribers). `test_placement.rs` registers the new
  `runtime_notification_subscriber_acceptance.rs` (31 → 32).
- `tddy-desktop` — new `native_notifications` module, `tauri-plugin-notification ~2.4`,
  `async-trait`, `tddy-daemon-kernel`. Detail:
  [native-notifications.md](../../../packages/tddy-desktop/docs/native-notifications.md).
- `flake.nix` — darwin-only `tddy-objc-cc` wrapper for the Objective-C that `mac-notification-sys`
  builds.

**Decisions (orchestrator, developer AFK).**

- No Tauri bump; clicks are not delivered. Wiring is
  `docs/dev/todo/2026-10-26-desktop-notification-click-needs-tauri-2-12.md`.
- No `notification:default` capability: only Rust calls the plugin.
- The fixed text of a daemon-worded notification is shown whole; only an agent body is cut to 300.
- An unresolvable OS user fails the desktop's start rather than notifying everyone or no one.
- **Pending-question suppression by `request_id` (wrap, 2026-10-28).** A status report following a
  recorded question is republished with the question attached and different text, so the
  identical-notice rule showed the question twice. A notice whose `QuestionNotice.request_id`
  equals the one the session's last shown notice was about is now skipped; pinned by
  `a_question_already_shown_is_not_shown_again_when_its_wording_changes` and
  `a_new_question_from_the_same_session_is_shown`.

**Growth recorded, not split** (n5–n8 also touch these; all under the 500-line budget):
`tddy-desktop/src/lib.rs` 428 → 467, `tddy-daemon/src/runtime.rs` 305 → 336,
`runtime/mount_host.rs` 362 → 401; new `native_notifications.rs` 310 production lines.

**Manual verification outstanding:** a real macOS notification (label first line, then body) — not
run unattended.
