# Changeset: Agent-written notifications

**Created:** 2026-10-25
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 5 — seven packages, a proto contract change (RPC + event fields), a new external dependency (`pulldown-cmark`), spawn/resume environment changes, Telegram HTML rendering with chunking
**Stack:** `#home-inbox 1/8` — branch `feature/home-inbox/agent-notify`, base `feature/start-parity/spawn-conversation` (#758, `#start-parity 9/9`)
**PRD:** [PRD-2026-10-25-agent-notifications.md](../../ft/daemon/1-WIP/PRD-2026-10-25-agent-notifications.md)

## Initial Discovery

[2026-10-25-home-inbox-agent-notify-initial-discovery.md](./2026-10-25-home-inbox-agent-notify-initial-discovery.md)

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-08-29-session-notifications-three-follow-ups-left-open.md](../todo/2026-08-29-session-notifications-three-follow-ups-left-open.md) | ⚠ During — this adds structured fields to `SessionNotification`, not the keyboard/routing policy the first item asks for. Do not fold the presenter path onto the bus here |
| [2026-09-12-tddy-session-activity-has-no-tests-of-its-own.md](../todo/2026-09-12-tddy-session-activity-has-no-tests-of-its-own.md) | ⚠ During — the new RPC's acceptance tests live **in `tddy-session-activity/tests/`**, so this crate gains its first own suite. The parity-oracle gap the entry names stays open |
| [2026-09-22-telegram-control-plane-left-over-budget-by-a-move-only-node.md](../todo/2026-09-22-telegram-control-plane-left-over-budget-by-a-move-only-node.md) | ⚠ During — Telegram rendering goes in a **new module**; `telegram_notifier.rs` and the `telegram_session_control/*` modules must not grow |
| [2026-10-17-desktop-cursor-hooks-reach-daemon.md](../todo/2026-10-17-desktop-cursor-hooks-reach-daemon.md) | ⚠ During — on an embedded daemon the notify env must carry `TDDY_DAEMON_SOCKET` (as claude-cli hooks do); cursor-cli gets the same env, so its notify works on the desktop even though its hooks still don't |
| `packages/tddy-telegram-control/docs/code-issues/oversized-file-telegram-notifier.md` | ⚠ During — do not grow |
| `packages/tddy-tools/docs/code-issues/oversized-file-server.md` | ⚠ During — `notify` is a CLI subcommand in its own module, not in `server.rs` |

## Affected Packages

- [ ] [`tddy-daemon-kernel`](../../../packages/tddy-daemon-kernel/README.md): `AgentNotification` on `SessionNotification`, source `Agent`, `notification_markdown` renderer (`pulldown-cmark`)
- [ ] [`tddy-service`](../../../packages/tddy-service/README.md): `activity.proto` — `ReportOperatorNotification`, event fields, `SOURCE_AGENT`
- [ ] [`tddy-session-activity`](../../../packages/tddy-session-activity/README.md): the RPC handler and its builder
- [ ] [`tddy-tools`](../../../packages/tddy-tools/README.md): `tddy-tools notify`
- [ ] [`tddy-agent-launch`](../../../packages/tddy-agent-launch/README.md): notify env + guidance fragment on claude-cli/cursor-cli spawn and resume
- [ ] [`tddy-telegram`](../../../packages/tddy-telegram/README.md): `TelegramSender::send_html_message`
- [ ] [`tddy-telegram-control`](../../../packages/tddy-telegram-control/README.md): agent-notification delivery
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): `PeerRoutedActivity` delegates the new RPC (unrouted, like `ReportSessionStatus`); `ActivityPorts.daemon_instance_id` wired from `local_instance_id_for_config` (pass 2)
- [ ] [`tddy-tools-cli`](../../../packages/tddy-tools-cli/README.md): the `notify` subcommand in the `tddy-tools` binary, and its process-level suite (pass 2)

## Responsibility

- `tddy_daemon_kernel::session_notification::{AgentNotification { title, body_markdown, level, sound, link }, NotificationLevel}`;
  `SessionNotification.agent: Option<AgentNotification>`; `SessionNotificationSource::Agent`.
- `tddy_daemon_kernel::notification_markdown::{to_telegram_html(md) -> String, to_plain_text(md, max_chars) -> String}`
  (`pulldown-cmark`; safe subset: `b i code pre a`, everything else escaped).
- `tddy_daemon_kernel::agent_notification_limits::{validate_agent_notification(title, body) -> Result<(), String>}` — title 1–200 after trim, body ≤ 2,000.
- Proto: `ActivityService.ReportOperatorNotification(ReportOperatorNotificationRequest { session_id, hook_token, os_user, title, body_markdown, level, sound })`;
  `SessionNotificationEvent.{title = 7, body_markdown = 8, level = 9, sound = 10, link = 11}`; `SESSION_NOTIFICATION_SOURCE_AGENT = 4`.
- `tddy_session_activity::service::operator_notifications::report_operator_notification_impl` — the same session-type + hook-token checks as `ReportSessionStatus`, then `publish_for_session`.
- `tddy-tools notify <title> [--body <md>|-] [--level …] [--sound]` (`packages/tddy-tools/src/notify_cli.rs`), credentials from `TDDY_SESSION_ID`, `TDDY_OS_USER`, `TDDY_HOOK_TOKEN`, `TDDY_DAEMON_URL` | `TDDY_DAEMON_SOCKET`; exits non-zero with the reason.
- `tddy_agent_launch::notify_env::{notify_env_pairs(session_id, os_user, hook_token, daemon_url, daemon_socket)}` and the guidance fragment `agent_notify_guidance.md`, applied in claude-cli start/resume (and the env in cursor-cli start/resume).
- `tddy_telegram::sender::TelegramSender::send_html_message(chat_id, html, silent)`.
- `tddy_telegram_control::agent_notification_telegram::AgentNotificationTelegram` — renders and chunks (4,096 B) an agent notification; `TelegramDaemonHooks::wants_notification` admits `source == Agent` and skips the per-session dedupe for it.

**Published in pass 2 — deviations from the list above**, each because the real code needed it:

1. **Two more packages.** `tddy-session-lifecycle`: `PeerRoutedActivity` implements the generated
   `ActivityService` trait, which has no default methods, so it gains `report_operator_notification`
   (delegated to local, unrouted). `tddy-tools-cli`: the `tddy-tools` binary is built there —
   `Subcommand::Notify(notify_cli::NotifyArgs)` → `notify_cli::run_notify`.
2. **`tddy_session_activity::ActivityPorts.daemon_instance_id: String`.** The link
   `#/sessions/<id>?host=<instance>` needs the host id and no port carried it; wired as
   `svc_session_files_ports` wires its own.
3. **The CLI suite lives in `tddy-tools-cli`**, not `tddy-tools` — see Testing Strategy.
4. **`tddy_service::session_activity::notify_env::{SESSION_ID, OS_USER, HOOK_TOKEN, DAEMON_URL, DAEMON_SOCKET}`** —
   the variable names, shared by the launch that exports them and the CLI that reads them.
5. **Surface not named above:** `ReportOperatorNotificationResponse { ok }`; proto
   `enum NotificationLevel { UNSPECIFIED, INFO, SUCCESS, WARNING, ERROR }` (request field 6, event
   field 9); `agent_notification_limits::{MAX_TITLE_CHARS, MAX_BODY_CHARS}`;
   `tddy_tools::notify_cli::{NotifyArgs, NotifyLevel, run_notify}`;
   `tddy_agent_launch::claude_cli_spawn_steps::{AgentNotifyLaunch, AgentNotifyWiring, agent_notify_launch, resumed_agent_notify_launch}`
   (the seam the launch tests drive — spawning a real `claude` is not one);
   `tddy_telegram::sender::RecordedHtmlMessage` + `InMemoryTelegramSender::recorded_html()`;
   `AgentNotificationTelegram::{messages, silent}` and `TELEGRAM_HTML_MESSAGE_MAX_BYTES`.
6. **Not published:** a `notification_for_agent` builder (internal to green). The start, resume and
   cursor-cli call sites are not wired yet — the step functions are `pub`, so nothing is dead code.
7. **Proto fields are Rust-only for now.** `activity_pb.ts` is regenerated
   (`scripts/generated-code.sh write packages/tddy-web packages/tddy-rust-typescript-tests`); the
   web reads none of the new fields, per Boundaries.

## Boundaries

- No desktop delivery (`#home-inbox 4/8`), no pending-question fields (`3/8`), no project log (`7/8`).
- No MCP `NotifyOperator` tool; the home main cannot notify (follow-up TODO).
- No sandboxed claude-cli support (it has no hooks or hook token).
- The web drawer is unchanged; it ignores the new fields.
- `session-hook` keeps its fail-quiet contract; `notify` is a separate subcommand.

## Dependencies

None in this stack.

**Base (replan 2026-10-26).** The base is now `#start-parity 9/9` (#758). `#term-snapshot` merged on
2026-10-25, and this stack continues the `#start-parity` line, so it lands after that stack.
This node consumes nothing from it. It touches no project-home tool registration, so it has no
rebase conflicts with `#start-parity`. `#start-parity 6/9` (#755) added
`ReportSessionStatusRequest.message = 5` to `activity.proto`. This node's RPC and its
`SessionNotificationEvent` fields sit beside it, and no field numbers collide. The `#start-parity`
branches predate `#coagents` on `master`, so `mount_host.rs` and `exec_tool.rs` take a mechanical
rebase once that stack is rebased.

## Draft PR contract

The first push (pass 2) publishes: the kernel types and the two renderer/validator signatures
(bodies `// TODO(agent-notify): implement`), the proto RPC and fields (regenerated), the
`ReportOperatorNotification` handler signature, `notify_env_pairs`, `TelegramSender::send_html_message`
on the port (in-memory sender implements it), and the failing tests below.

## Green wave

**Wave:** 1 of 4
**Greenable independently:** yes — every test drives this node's own RPC, CLI, renderer and the
in-memory Telegram sender; nothing exercises a predecessor's behaviour.
**Concurrent with:** `#home-inbox 2/8` terminal-read
**Blocks:** `3/8` elicit-content (notification model), `4/8` desktop-notify (model + `to_plain_text`), `7/8` home-wait (model)

Real dependency edges, as opposed to the branch line:

    n1 → n3, n4, n7      n2 → n5      n3 → n6, n7      n6 → n8      (cross-stack, replan 2026-10-26: #start-parity #750 [merged] → n2, n5, n6, n7, n8 · #755 → n3 · #751, #758 → n6 · #coagents [merged] → n6)

## State A

- `SessionNotification { session_id, os_user, label, kind, source, text, at_unix_ms }` — `text` is
  final prose; three publishers, fixed copy.
- Telegram: `send_message(chat_id, text)` with no `parse_mode`; the notification path never chunks;
  dedupe on `(session_id, text)`; admits only `AttentionRequired` from `ActivityStatus`.
- claude-cli sessions hold a hook token (`.session.yaml`, `.claude/settings.local.json`) used only by
  `tddy-tools session-hook` (fail-quiet, 2 s, silent stdout). Their process env carries no daemon
  credentials. Plain claude-cli gets no appended system prompt.

## State B

- An agent runs `tddy-tools notify "…" --body "…"`. The CLI reads the env, calls
  `ReportOperatorNotification` over the socket or HTTP, and prints `Notification sent` or exits 1 with
  the daemon's reason.
- The daemon validates and publishes `{kind: AttentionRequired, source: Agent, agent: Some(…), text: "<title>"}`;
  the drawer lights yellow; Telegram renders `<b>title</b>\n<i>label</i>\n\n<body html>\n\n<link>`,
  chunked, `disable_notification = !sound`.

## Delta

### New
- Kernel `AgentNotification`, `NotificationLevel`, `notification_markdown`, `agent_notification_limits`; dependency `pulldown-cmark` (tddy-daemon-kernel)
- Proto RPC + fields; `operator_notifications.rs` (session-activity); `notify_cli.rs` (tddy-tools); `notify_env.rs` + `agent_notify_guidance.md` (agent-launch); `agent_notification_telegram.rs` (telegram-control)
### Modified
- `SessionNotification` (+`agent`), `streams::session_notification_event` (maps the new fields)
- `TelegramSender` (+`send_html_message`), `TelegramDaemonHooks::{wants_notification, on_notification}`
- claude-cli/cursor-cli start and resume: env pairs; claude-cli `--append-system-prompt-file` composes the guidance fragment with any managed-recipe prompt
- `docs/ft/daemon/{session-notifications.md,telegram-notifications.md,claude-cli-session.md}` at wrap

## Testing Strategy

### Acceptance tests

**`tddy-session-activity`**: `packages/tddy-session-activity/tests/operator_notification_acceptance.rs`
- [x] `an_agent_notification_from_a_claude_cli_session_is_published_with_its_title_body_level_sound_and_link`
- [x] `a_title_empty_after_trimming_or_over_200_chars_or_a_body_over_2000_chars_is_refused_and_publishes_nothing`
- [x] `a_wrong_hook_token_is_refused_as_permission_denied`
- [x] `a_session_that_is_not_claude_cli_or_cursor_cli_is_refused`

**`tddy-telegram-control`**: `packages/tddy-telegram-control/tests/agent_notification_telegram_acceptance.rs`
- [x] `an_agent_notification_reaches_its_chats_as_html_with_the_title_bold_and_the_link_last`
- [x] `a_body_over_4096_bytes_arrives_in_several_html_messages_each_well_formed`
- [x] `without_sound_the_message_is_sent_silently`
- [x] `two_identical_agent_notifications_are_both_delivered`

**`tddy-tools-cli`**: `packages/tddy-tools-cli/tests/notify_cli_acceptance.rs` — process-level
against the real binary, which `tddy-tools-cli` builds (`tddy-tools` is a library); it reuses the
agent-tool-socket harness of `session_hook_cli.rs` beside it
- [x] `notify_reads_its_credentials_from_the_environment_and_reports_success` (fake ActivityService on a socket)
- [x] `notify_exits_non_zero_naming_the_reason_when_the_daemon_refuses`
- [x] `notify_exits_non_zero_when_no_session_environment_is_present`

**`tddy-agent-launch`**: `packages/tddy-agent-launch/src/claude_cli_spawn_steps/claude_cli_spawn_steps_tests.rs`
- [x] `a_claude_cli_session_is_launched_with_the_notify_environment_and_the_guidance_fragment`
- [x] `a_resumed_claude_cli_session_carries_the_notify_environment_again`
- [x] `an_embedded_host_exports_the_daemon_socket_not_the_url`

### Unit tests

- [x] `tddy-daemon-kernel` `notification_markdown`: bold/italic/code/pre/link render; `<script>`, raw HTML and `&` are escaped; nested lists flatten to text; `to_plain_text` strips markup and cuts at a char boundary with `…`.
- [x] `agent_notification_limits` boundaries (0, 1, 200, 201 chars; 2,000 / 2,001).
- [x] `tddy-agent-launch` `notify_env_pairs`: URL host vs socket host (socket replaces the URL);
  the guidance and a managed recipe's prompt composed into the one appended file.
- [x] `tddy-telegram-control` `AgentNotificationTelegram::render`: fixed layout, title-only, title
  and label escaped, `silent == !sound`.

Files: `packages/tddy-daemon-kernel/tests/{notification_markdown_unit,agent_notification_limits_unit}.rs`,
`packages/tddy-agent-launch/src/notify_env_unit_tests.rs`, the last test of
`claude_cli_spawn_steps_tests.rs`, `packages/tddy-telegram-control/tests/agent_notification_telegram_unit.rs`.
`validate_agent_notification`'s stub returns an `Err` so its accepting cases fail too.

### Test level decisions
| Aspect | Level | Rationale |
|---|---|---|
| RPC auth + publish | Integration over the activity service with a recording subscriber | The contract is the RPC |
| Telegram rendering/chunking | Integration with `InMemoryTelegramSender` | No network; asserts the exact HTML |
| CLI | Process-level against a fake socket server | The exit code and message are the agent's interface |

## Decisions & Trade-offs

- Superset model (developer, 2026-10-25): agents run a CLI with credentials from the environment; no
  MCP tool in this node.
- `pulldown-cmark` → Telegram HTML (developer, 2026-10-25).
- Guidance paragraph appended to daemon-launched claude-cli sessions (approved with the PRD,
  2026-10-25). cursor-cli gets the env only; its CLI has no system-prompt flag this node relies on.
- Limits are refusals, not truncation, so the agent learns its message was too long.

## TODO

- [x] Record initial discovery (`2026-10-25-home-inbox-agent-notify-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
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

## Pre-existing failures

Not this node's red — `/green` must not count them:

- `tddy-agent-launch --test clone_askpass_acceptance` panics "target/debug/tddy-askpass is not
  built" under a bare `cargo test`; a harness note, not a failure — `./test` (or
  `cargo build -p tddy-askpass` first) builds the helper.
- `tddy-agent-launch --lib session_acting_identity_tests::a_local_gh_project_yields_the_gh_logins_four_pairs_and_its_github_source`
  is flaky under a parallel package run (seen once: only `TDDY_GITHUB_SOURCE` came back); it passed
  on the baseline and passes alone. Untouched by this node.

## Technical Debt & Production Readiness

- Follow-up TODO to file at wrap: an MCP `NotifyOperator` for sessions without a shell (the project-home main).

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
