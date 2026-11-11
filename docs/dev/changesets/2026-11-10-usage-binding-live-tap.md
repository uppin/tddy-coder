# 2026-11-10 — Daemon-run coding subagents and stream-only agents report their usage

`#usage-binding 3/5`, PR [#892](https://github.com/uppin/tddy-coder/pull/892), stack
[#895](https://github.com/uppin/tddy-coder/stack/895). Feature:
[session-token-accounting.md](../../ft/coder/session-token-accounting.md).

## What changed

- **`tddy-agent-backend`** — `usage_tap` (`UsageTapSpec`, `UsageTapSpec::for_conversation`,
  `UsageTap::{open, offer_line, offer_value}`): the events a spec declares as usage, appended verbatim
  to `<session_dir>/usage/<conversation_id>.jsonl`; a failed write disables the tap for the turn and
  never fails it. `HeadlessCommand::usage_tap` (the headless driver offers every stdout line) and
  `OpenSession::usage_tap` (the ACP client offers every serialised `session/update` of a tapped
  session).
- **`tddy-coding-agent-specs`** — `headless.usage_events` and `acp.usage_events` (`Vec<FieldEq>`).
  `claude.yaml` taps its headless `result` and binds `total_cost_usd` as a second, cost-only source.
- **`tddy-usage`** — `registry` (`RegisteredConversation`, `register_conversation`,
  `registered_conversations`, `usage-conversations.jsonl`, last line per id wins); `merge_sources`
  (in `gather`, used by `account`): one row per conversation id across a binding's sources;
  `validate_usage_section` refuses two sources mapping one counter or both mapping `cost`;
  `gather_session_usage_for` evaluates every registered conversation under its own spec's binding,
  nested under its parent.
- **`tddy-coding-subagent`** — `CodingAgentLaunch::session_dir`; `UsageReading` and
  `CodingAgentSession::with_usage_reading`; `cumulative_usage` reads the conversation's row.
- **`tddy-session-agents`** — `open_coding_agent_session` registers the conversation under the
  session's main one (`register_usage_conversation`) and attaches its reading; the ACP open carries
  the tap from `acp.usage_events`.
- **`tddy-session-lifecycle`** — the daemon's headless launcher sets `HeadlessCommand::usage_tap` from
  `headless.usage_events`.

## Deviations from the plan

- **No Cursor binding.** No recorded Cursor `result` carries usage; a binding would be invented —
  [todo](../todo/2026-11-11-cursor-headless-usage-unbound.md).
- **No ACP cost bindings.** ACP `usage_update.cost` is cumulative (`agent-client-protocol-schema`
  0.11.4), so it needs a `last` aggregate; the ACP tap ships and is tested, but no spec binds it.
- **The verified binding is Claude's headless `result` cost**, which changed #890's
  `the_builtin_claude_specs_bind_their_transcripts` from "exactly one source" to "the first source is
  the transcript".
- **One tap in the headless driver, not per adapter.** The driver offers each raw stdout line, so
  stream-json, codex-jsonl and every other headless CLI share it; the planned per-adapter
  `headless_driver/usage_tap.rs` became the crate-level `usage_tap` the ACP driver also uses.
- **`context_tokens` stays 0** (`TODO(usage-binding)`): occupancy is one record's input plus cache
  counters, which no whole-counter aggregate (`sum` or `last`) answers.
- **The end-to-end subagent test** is
  `a_coding_subagent_the_daemon_opens_reports_its_own_usage_nested_under_the_session` in
  `tddy-session-agents/tests/headless_coding_agent_opens_as_a_headless_conversation.rs` (it reuses
  that suite's fake claude), not a separate `subagent_usage_acceptance.rs`; it drives the tap, the
  registry and the stream's gather with the built-in claude binding. The roster's own registration
  is unit-tested in `coding_agent.rs`.

## Wrap fixes

- `UsageTapSpec::for_conversation` owns the tap path, which the ACP and headless launch paths each
  spelled out.
- `merge_sources` moved from `registry.rs` to `gather.rs`, beside its one caller.
- `headless_driver.rs`'s stderr reader moved to `headless_driver/stderr.rs` (see *Code issues*).

## Code issues

- `tddy-agent-backend` `oversized-file-headless-driver` — **closed**: 505 → 515 with the tap call and
  `HeadlessCommand::usage_tap`, then **474** production lines after `drain_stderr` / `stderr_tail`
  moved to `headless_driver/stderr.rs` (`tddy-tools restructure` `move_item`; the import tidy was
  done by hand). Record deleted.
- `tddy-session-lifecycle` `coding_agent_launcher/headless_tests.rs` (a `#[cfg(test)]` module file,
  640 → 641, one `session_dir: None`): test code, not a budget finding.
- No other code issue in a touched package changed; no stack-shared file was decomposed (`tddy-usage`
  `gather.rs`, `lib.rs` and `section.rs` are also modified by `codex-rollout` / `opencode-sqlite`, and
  all are under 500).

## Left for later

- A `shared`-placement ACP subagent is registered but never tapped (its launch has no
  `conversation_id`) — [todo](../todo/2026-11-11-shared-acp-conversation-usage-untapped.md).
- Cursor's headless usage — [todo](../todo/2026-11-11-cursor-headless-usage-unbound.md).
- ACP cost bindings wait for `aggregate: last` (`#usage-binding 4/5`).
