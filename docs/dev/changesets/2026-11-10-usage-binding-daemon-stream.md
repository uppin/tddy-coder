# 2026-11-10 — The daemon accounts every session's usage, and the Usage tab shows all of it

`#usage-binding 2/5`, PR [#891](https://github.com/uppin/tddy-coder/pull/891), stack
[#895](https://github.com/uppin/tddy-coder/stack/895). Features:
[session-usage-inspector.md](../../ft/web/session-usage-inspector.md),
[session-token-accounting.md](../../ft/coder/session-token-accounting.md).

## What changed

- **`tddy-usage`** — `UsageContextFile`, `write_usage_context` / `read_usage_context`
  (`<session_dir>/usage-context.json`) and `gather_session_usage_for`.
- **`tddy-agent-launch`** — `usage_context` (`UsageSubject`, `record_usage_context`): the host PTY
  start and resume record the OS user's home; the jail records `grants.home` in `launch_jailed`, so
  its start and relaunch both do. `LaunchConversation::usage_id`; `HostCodingAgents::usage_bindings`.
- **`tddy-spawn`** — `plan_session_child` records the workflow session's context when given an agent.
- **`tddy-service`** — `activity.proto`: `StreamSessionUsage`, `StreamSessionUsageRequest`,
  `SessionUsageSnapshot`, `UsageConversation`. `remote.proto`: `TokenUsageUpdated` and
  `ConversationRecord` removed, field 13 and `token_usage_updated` reserved; the `convert.rs` arm and
  test removed. TS regenerated in `tddy-client-web` and `tddy-rust-typescript-tests`.
- **`tddy-session-activity`** — the handler and `UsagePollers` (one poller per watched session, 2 s
  tick, publish on change, the relay leaves on a dropped stream, the last subscriber stops the
  poller); the `ActivityPorts::usage_bindings` port.
- **`tddy-session-lifecycle`** — `PeerRoutedActivity::stream_session_usage` (refused when addressed
  at a peer) and `usage_bindings` from the host's coding agents.
- **`tddy-session-web`** — `useSessionUsage` on the daemon's `ActivityService` client;
  `SessionUsageTab` columns, tree indentation and cost cells; `buildUsageTree`, `usageTotals` with
  `costPartial`; the drawer's `serverIdentity` prop removed.
- **Deleted** — `tddy-presenter::usage_watcher` and its two test suites,
  `PresenterEvent::TokenUsageUpdated`, the `tddy-tui` arm, `tddy-coder`'s watcher spawn,
  `SESSION_USAGE_POLL_INTERVAL`, `resolve_agent_home_dir` and its `tddy-usage` dependency.

## Deviations from the plan

- **The context file lives in `tddy-usage`**, not `tddy-agent-launch`: the activity service reads it,
  and depending on the launch crate for that would invert the graph.
- **The context records `model`**, the main row's fallback: the stream has no CLI args to take it from.
- **No peer forwarding.** A request addressed at a peer is refused, exactly as `StreamSessionActivity`
  is; the web opens the stream on the session's own host. The PRD's "routed to its owner" is narrowed
  to that.
- **Cost renders `$x.xx`** (the PRD), not the 4-decimal form the changeset once sketched.
- **The daemon end-to-end test** lives in `tddy-session-lifecycle`
  (`tests/terminal_session_usage_acceptance.rs`, the real `DaemonSessionHost`), not `tddy-daemon`. Its
  binding reads `{session_dir}`, so no fixture is written into the OS user's real home.
- **The relaunch test became a host resume test** (`a_resumed_session_keeps_its_usage_context`); the
  jail relaunch records through the same `launch_jailed` path the jailed start test covers.
- **The poller-stops behaviour is unit-tested** in `usage_pollers.rs`, including a dropped stream.
- **Two Cypress tests reworded** in `CreateSessionAndStreamsVisibleErrorsAcceptance.cy.tsx`: "no room"
  became "no daemon client yet", and "room still joining" was dropped — the tab no longer reads a
  room, so that state does not exist for it.

## Wrap fixes

- **Poller leak**: the subscriber relay waited on the next change before noticing a dropped client,
  so a session whose usage stopped changing was polled forever after its tab closed. The relay moved
  into `UsagePollers::stream` and selects on the client's channel closing.
- `HostCodingAgents::usage_bindings` took the registry lookup out of `svc_activity_ports.rs`, which
  had crossed the 500-line budget (478 → 503 → **495**).
- `tddy-spawn`'s context write extracted to `record_workflow_usage_context` (`plan_session_child`
  180 → 214 lines inline, 182 after).
- `tddy-coder`'s unused `tddy-usage` dependency dropped; stale watcher comments in `run.rs` and
  `virtual_tui.rs` removed; the activity stream's lock tolerates poisoning.

## Code issues

- `tddy-agent-launch`: six length records **regressed** by the context write — `launch_jailed`
  151 → 161, `start_cli_harness_session` 303 → 310, `resume_cli_harness_session` 118 → 128,
  `relaunch_sandboxed_cli_harness` 140 → 144, `start_sandboxed_cli_harness_session` 286 → 287,
  `launch_jailed_cli_agent` 82 → 84. Not split; a row added to each.
- `tddy-coder` `oversized-file-run` — 2,511 → 2,458, better, not clean.
- `tddy-tui` `oversized-file-virtual-tui` — 963 → 961.
- `tddy-usage` `context_file.rs` and `tddy-session-activity` `usage_pollers.rs`: no CRAP finding
  (top context-file function CRAP 3.0; no `usage_pollers` function in the activity crate's top 50, whose
  floor is 6.0).

## Left for later

- A workflow spawn without `--agent` records no usage context —
  [todo](../todo/2026-11-11-workflow-spawn-without-agent-records-no-usage-context.md).
- An agent with no conversation id the daemon knows has no main row (the `#usage-binding 1/5`
  engine needs one); `tddy-coder`'s session participant does not serve `StreamSessionUsage`.
