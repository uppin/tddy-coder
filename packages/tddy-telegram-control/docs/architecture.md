# tddy-telegram-control architecture

## Overview

The crate is the Telegram front-end of the daemon's sessions. Inbound, `telegram_bot` long-polls
the Bot API and hands each message and callback to `TelegramSessionControlHarness`, which writes
session directories (`changeset.yaml`), spawns sessions through `tddy-session-lifecycle`, and sends
presenter input over `PresenterIntent`. Outbound, `TelegramSessionWatcher` turns a session's
presenter events and metadata ticks into chat messages and keyboards.

It sits above both `tddy-telegram` and `tddy-session-lifecycle` and is reached by
`tddy-daemon` alone — see [README.md](../README.md) for why it is a crate of its own.

## `telegram_session_control/` — ten files, one harness

`mod.rs` holds `TelegramSessionControlHarness` and the inherent methods more than one flow calls; its
DTOs live in `types.rs`, glob-re-exported from `mod.rs` so every child sees them under the same names.
Six child modules (`session_start`, `cli_spawns`, `pickers`, `workflow_launch`,
`chaining_and_listing`, `elicitation`) each add one `impl TelegramSessionControlHarness` block for one
family of flows. A private method is
visible only inside the module that declares it and its children, so a helper two families share
lives in `mod.rs`, where every child can see it; a free helper they share is `pub(super)` in
`callbacks` or `workflow_spawn`. Those two hold no harness methods: `callbacks` is pure functions,
and `workflow_spawn` is `TelegramWorkflowSpawn` and its helpers.

| Module | Prod lines | Holds |
|---|---:|---|
| `callbacks.rs` | 736 | the command and `callback_data` vocabulary: the slash commands, the prefixes, and the pure parsers, formatters and chunkers (`parse_*`, `chunk_telegram_text`) with their unit tests |
| `pickers.rs` | 617 | the project, branch-intent, branch, branch-conflict, model and agent keyboards, which end in `spawn_telegram_workflow` |
| `chaining_and_listing.rs` | 518 | `/chain-workflow` and its parent picker, recipe and plan-review callbacks, the session list with Enter and Delete |
| `workflow_spawn.rs` | 480 | `TelegramWorkflowSpawn`: the chain integration-base merge, the spawn configuration and inputs, and the branch-name helpers the start paths share |
| `elicitation.rs` | 456 | presenter input from Telegram: feature submission, document review, elicitation answers, the changeset writes and recipe keyboard around them |
| `cli_spawns.rs` | 444 | the Claude and Cursor model-pick callbacks and the two CLI spawns they end in, `spawn_telegram_claude_cli` and `spawn_telegram_cursor_cli` — including the terminal exposure a Claude start announces, where the exposure port's `FailedPrecondition` ("nothing is wired") is not logged as a failure |
| `mod.rs` | 413 | the harness and the shared helpers |
| `session_start.rs` | 296 | `/start-workflow`, `/start-claude`, `/start-cursor`: session creation up to the model pick |
| `types.rs` | 151 | the harness's DTOs |
| `workflow_launch.rs` | 133 | `spawn_telegram_workflow`: from the session's recorded recipe and chain parent to the "Workflow started" message — the spawn deps, the project lookup, the chain child's integration-base merge, the spawn, following the spawned session's presenter stream, and the announcement |

Production lines are counted to the first `#[cfg(test)]`. The repo's file budget is 500; no test
enforces it (line-budget tests were removed 2026-10-26). The three modules over it each have a record in
[code-issues/](code-issues/) naming its seams. Two of them (`pickers.rs` and the production half of
`callbacks.rs`) cannot reach 500 by moving items alone, because their size is in long functions such
as the 177-line `handle_telegram_branch_callback`.

## GitHub account of a Telegram-started session

A Telegram start has an OS user and a Telegram-to-GitHub-login link but **no session token**, and the
credential vault opens only by an owner's session token. Reaching an already-open vault *by login*
would let a Telegram chat bypass that gate, so no mechanism exists: a session started here starts under
the checkout's own identity (no `GIT_*` pairs) and **no GitHub token is available to it**. The
harness's `notify_account_identity_unavailable` makes that visible: when the project assigns an
account, the chat is told, after the start message on the workflow, claude-cli and cursor-cli paths,
that the account's commits and the agent's GitHub tools are unavailable and why, and that starting from
the web dashboard acts as the account; the same text is logged at `warn`. A project assigning no account
is told nothing, since nothing is lost. Product view:
[per-project GitHub identity](../../../docs/ft/daemon/github-identity.md).

A **Local gh** project needs no vault, so its Telegram start does carry an identity:
`workflow_spawn::telegram_git_environment(project, os_user, &GithubCli)` resolves the host's `gh` login
as the OS user (`ProjectGithub::LocalGh`) into the same four pairs and `TDDY_GITHUB_SOURCE` a dashboard
start carries, and `account_identity_unavailable_notice` returns `None` for it: nothing is
unavailable, so the "start from the web dashboard" notice is not sent. A project that is not Local gh
keeps the behaviour above. Limit: the `GithubCli` is built here (`GithubCli::on_spawn_child_path()`),
so the host's injected one does not reach it; see
[the open TODO](../../../docs/dev/todo/2026-10-15-local-gh-child-sessions-and-telegram-gh.md).
This crate depends on `tddy-host-service` for it.

A Telegram-started *tool* session is also started with **no host-session socket**: the socket answers
from a registration the daemon's own session host makes at start, and this crate does not hold that
host. Without a registration the socket would only refuse, so the flag is left off and the agent's
request is refused as having no credential handler. The refusal is explicit rather than silent, and
the open work is marked `TODO(stdio-relay)` in `workflow_spawn.rs` (the daemon's session host and its
sockets would have to be handed to this crate). Telegram never wired `spawn_conversation`, so this is
not a regression.

## Telegram's operator channel

The daemon's core reaches an operator's chat through one port,
`tddy_daemon_kernel::operator_channel::OperatorChannel`
([`daemon-kernel.md` § Ports](../../tddy-daemon-kernel/docs/daemon-kernel.md#ports)). Telegram's
implementation is `TelegramDaemonHooks` (`telegram_session_subscriber`), built with
`TelegramDaemonHooks::new(config, sender, watcher)`. It holds the daemon's `DaemonConfig`, the shared
`TelegramSender` and the `TelegramSessionWatcher`, plus a private per-session dedupe of the last
notification text sent (`LastDeliveredPerSession`). Its `id()` is `telegram`
(`tddy_telegram::status::TELEGRAM_INTEGRATION_ID`).

| Port method | What Telegram does |
|---|---|
| `on_presenter_event` | locks the watcher and calls `on_server_message`. An error ends that session's presenter observer. The watcher opens and closes its per-chat elicitation queue on the core's `awaits_operator` rule, so it agrees with the drawer indicator on every mode |
| `wants_notification` | `AttentionRequired` from the `ActivityStatus` path (unless it carries a `question` — the pending question already alerted), from an agent (`Agent`) or a claude-cli pending question (`PendingQuestion`), while the `telegram:` block is enabled. `Activity` is declined (every tool call would become a message) and so is `Presenter` (those elicitations already arrive through `on_presenter_event`, with their keyboards — taking them twice would double-send) |
| `on_notification` | a `PendingQuestion` is rendered by `question_alert::QuestionAlert` and sent loud as HTML once per `request_id` (the dedupe key `question:<request_id>`, apart from status texts); an agent-written notification is rendered by `agent_notification_telegram::AgentNotificationTelegram` and sent as HTML (never de-duplicated); otherwise drops a text identical to the last one sent for that session, resolves the recipients tracked-first (the chats that entered the session, else the configured broadcast list; an unreadable tracking map sends nothing), and sends the notification's text to each, failing with "N of M chat(s)" when some sends fail |

`agent_notification_telegram.rs` is a module of its own because rendering is a pure function of the
notification: `AgentNotificationTelegram::render` lays out `<b>title</b>`, `<i>label</i>`, the body
through `tddy_daemon_kernel::notification_markdown::to_telegram_html` and the link, then splits it
into messages of at most `TELEGRAM_HTML_MESSAGE_MAX_BYTES` (4,096) — at whitespace, never inside a
tag, entity or character, closing the tags open at a break and reopening them in the next message.
`send_to` sends every message to each chat through `TelegramSender::send_html_message` with
`silent = !sound`, and stops a chat at its first refused message.

`question_alert.rs` is the same kind of pure renderer for a claude-cli pending question: a permission
as `<i>Session {label} wants to run</i> <code>{tool}</code>: <code>{summary}</code>`, a question set as
each question's bold header, text and `• label: description` options. It escapes only `&`, `<` and `>`
(the kernel's `escape_html` also escapes `"`, which the layout does not want) and splits through
`agent_notification_telegram::split_html` (`pub(crate)`), so a long question set is several
well-formed messages; `QuestionAlert::html()` is the whole alert and `messages()` what is sent.

`tddy-daemon`'s `runtime/mount_host.rs` installs the hooks twice from one value: on
`DaemonSessionHost::new` as its `SharedOperatorChannel`, and on the daemon's notification bus through
`tddy_session_activity::session_notifications::OperatorChannelSubscriber`, beside the
`StreamSessionNotifications` relay. A daemon with no bot installs `NoOperatorChannel` in both
places. The presenter observer loop itself — connect with retry, read the stream, publish to the bus —
is `tddy-session-activity`'s and runs whether or not Telegram is configured, because the bus half
lights the drawer indicator in `tddy-web` on every daemon. A test that wants the channel on a host's
bus installs the bus the way the daemon does (`telegram_claude_cli_activity_alert_acceptance`).

The sender the hooks hold is a `tddy_telegram::status::ReportingSender` over the `TeloxideSender`, so
every send the channel makes reports Telegram's state on the integration status board. The inbound
control plane's own sender (`telegram_bot`) is a plain `TeloxideSender` and does not report; the bot's
identity check before the dispatcher starts covers token health.

The Telegram-started spawn path (`workflow_launch.rs`, `spawn_telegram_workflow`) starts the same
observer with the hooks as its channel and **no** bus. A Telegram-started workflow session therefore
raises no drawer indicator. That is recorded as a `TODO(session-notifications)` at the call site:
publishing needs the bus and the sessions base on `TelegramWorkflowSpawn`, which every inbound-control
harness constructs by hand.

## Log targets

Log targets keep their `tddy_daemon::` names: `tddy_daemon::telegram`,
`tddy_daemon::telegram_bot`, `tddy_daemon::telegram_session_control`,
`tddy_daemon::telegram_multi_select_shortcuts` and `tddy_daemon::session_notifications`. An
operator's `log:` filter in `daemon.yaml` is an observable interface, so the crate a line comes from
does not show in its target.

## Tests

The 17 suites in `tests/` exercise the crate through its own paths
(`tddy_telegram_control::…`). `telegram_operator_channel_acceptance` pins the channel's port contract
(its id, what it wants and declines, delivery to the tracked chat, one send per unchanged alert);
`telegram_notification_subscriber_unit` pins the routing matrix and the dedupe;
`telegram_workflow_launch_acceptance` covers the workflow launch's refusals (a daemon without LiveKit
is refused with the reason and announces nothing; a chain child of a gone parent is refused at the
base merge). The Claude and Cursor start suites wait on a PTY with
`tddy_testing_commons::wait::a_capture_showing` and its `PTY_STUB_OUTPUT` ceiling, shared with
`tddy-session-lifecycle`'s CLI suites.

The crate's shape — the port, the manifests, the module budget — is pinned from
`packages/tddy-daemon-kernel/tests/telegram_extraction_shape.rs`, beside the port it asserts on. The
workspace rule that only this crate, `tddy-telegram` and `tddy-daemon` name a Telegram crate is
`packages/tddy-daemon-extensions/tests/telegram_is_a_satellite.rs`.
