# 2026-11-24 — A Cursor TUI turn is submitted, and a resume keeps the model picked inside the agent

**Type:** Fix

Branch `fix/cursor-tui-submit-and-resume-model`. Packages: `tddy-tool-engine`,
`tddy-session-driver`, `tddy-daemon-rpc`, `tddy-coding-agent-specs`, `tddy-session-activity`.

## Problem

Observed 2026-11-24 with Cursor Agent 2026.11.19 in tmux on a Coder workspace target, and locally:

1. **Text and Enter sent together were not submitted.** `SendSessionKeys(text, keys: [Enter])`,
   `SendSessionMessage` and `ResumeSession(message)` left the text in Cursor's input box; a second,
   separate Enter submitted it. Claude Code submits the identical bytes.
2. **A model picked inside the TUI was lost on resume.** `/model` → GLM 5.3 Flash, then a resume
   relaunched the agent with the start's `--model auto`.

### Root cause 1 — bytes trailing a paste end wait for the next input

Every delivery path encoded the text and the submit key into one buffer and one write
(`encode_keys` → `send_input`): `ESC[200~ text ESC[201~ \r`. Cursor's input handler (bundled
`5548.index.js`) inserts the paste at `ESC[201~` and stores the **rest of that read**
(`xt.current = remainder`), prepending it to the *next* input event — so the `\r` waits for another
key. Unbracketed, a multi-byte read is a paste burst held for 75 ms (`T=75`), and a `\r` inside the
window joins it as a newline.

Live matrix (a pty harness writing exact chunks; SUB = submitted, LEFT = left in the box):

| Bytes | raw pty | local tmux | `coder ssh` → remote tmux |
|---|---|---|---|
| A — paste + `\r`, one write (what delivery did) | LEFT | LEFT | LEFT |
| B — paste, then `\r` as a second write, 0 ms apart | SUB | SUB | **LEFT** |
| C/D/E — `\r` 10 / 50 / 200 ms after the paste | SUB | SUB | SUB |
| F — paste + `\n`, one write | LEFT | LEFT | LEFT |
| G — raw text + `\r`, one write | LEFT | LEFT | LEFT |
| H — raw text, `\r` 50 ms later | LEFT | LEFT | LEFT |

Row B is why splitting the write is not enough: a transport in between merges writes made back to
back into one read. Claude Code submits row A.

### Root cause 2 — the record's model was written once, at start

Every resume path renders `--model` from `.session.yaml` `model`
(`tddy-agent-launch`'s `opencode_cli_spawn/resume.rs`, `svc_resume_sandboxed_cli_harness_session.rs`),
and nothing updated it after the start. Dropping `--model` on resume is no fix: both agents' `/model`
rewrite a **global** default (`~/.cursor/cli-config.json`, `~/.claude/settings.json`), not the
session's. Every Cursor hook event carries the session's current `model` (`glm-5p3-flash-high` after
the pick; `agent --model glm-5p3-flash-high` accepts it; `auto` reports itself as `auto-smart`), and
it already reached `ReportSessionStatus.hook_fields`, unread. Claude Code's hooks name the model on
`SessionStart` only; its transcript names each assistant message's `message.model`.

## Fix

- **Keys wait for the echo** (`tddy-tool-engine::terminal_keys`): `encode_typing` keeps the text and
  the keys as two writes (`Typing`); `type_into` writes the text, waits until the terminal's output
  offset has moved past where it stood and held still for 30 ms (capped at 500 ms after the echo
  began), then writes the keys. No echo within 5 s: the keys are pressed anyway — operator decision
  2026-11-24 — with a `warn` log and `echo_seen: false`. `TypingTerminal` is the port, with three
  adapters: `LeasedTerminal` (`SendSessionKeys`), `LiveTerminal` (`tddy-daemon-rpc`'s
  `paste_and_submit_to`: `SendSessionMessage`, `ResumeSession`'s first message, the watch waker) and
  `DriverSession` (`tddy-session-driver`'s `send_keys`). `TypingError` says how far typing got.
- **Answers gain `echo_seen`**: `SendSessionKeys` `{sent_bytes, echo_seen?}` (when keys followed text),
  `SendSessionMessage` for `terminal_paste`, `ResumeSession` when a first message was pasted.
  `TurnDelivery::TerminalPaste { echo_seen }`, `ResumeOutcome::Resumed { message }`.
- **The record follows the reported model**: a `coding_agent.model_report` spec slot
  (`hook_field` / `usage` sources, `events`, `ignore`; refused at load for a field-less hook source
  or a usage source without a `usage` binding). `cursor.yaml` reads `model` off `sessionStart`,
  `beforeSubmitPrompt` and `stop`; `claude.yaml` off `SessionStart`, then the transcript on `Stop`
  ignoring `<synthetic>`. `tddy-session-activity`'s `follow_reported_model` runs after
  `follow_conversation_rotation` in `ReportSessionStatus`. Resume code is unchanged: it reads the
  record. Cursor's `auto` is recorded as reported (`auto-smart`) — operator decision 2026-11-24;
  `agent -p … --model auto-smart` was verified to run.
- **A wake holds no project watch state while it pastes** (`tddy-session-activity`): the echo wait
  can take 5 s, and `deliver_wake_with` ran its delivery under the project's watch-state lock, which
  would have blocked every create, list, renew, collection and stream call of the project for that
  long. The delivery is planned on a copy (`plan_delivery` → `PlannedDelivery { delivery, commit:
  DeliveryCommit }`), the lock dropped, and a successful paste committed (`commit_delivery`: forward
  only; handed endings taken; a watch cancelled meanwhile stays gone). A per-owner delivery guard,
  held by the wake, `take_delivery` and `take_stream_batch_picking`, keeps what a wake is pasting
  from being handed to the same owner twice.
- **The waker wakes each orchestrator in a task of its own** (`tddy-daemon-rpc` `run_watch_waker`):
  `in_flight` / `due_after_flight` per owner; one orchestrator that never echoes no longer delays
  another's wake, nor the cue loop.

## Review fixes (2026-11-24)

From the branch's validation pass, all applied before the wrap:

- The waker was one serial task for every owner on the host, so one non-echoing orchestrator
  delayed every other owner's wake by up to 5 s (5.1 s late in the acceptance test on the serial
  waker): each wake is now its own task, pinned by
  `an_orchestrator_that_never_echoes_does_not_hold_up_anothers_wake`.
- `follow_reported_model` wrote a copy of `.session.yaml` read before an await (the transcript
  parse), which would overwrite a status or rotation written meanwhile: the record is re-read just
  before the model is written.
- Cursor's `model_report` read every hook event, so a subagent's tool call on another model could
  re-point the session: limited to `sessionStart`, `beforeSubmitPrompt`, `stop`, with a test.
- Tests added: the settle cap, `TypingError::NothingSent` / `KeysNotSent`, raw multi-line
  `encode_typing`, a stream call during a wake, a watch cancelled during a wake.
  `spec_model_report_acceptance.rs` renamed `spec_model_report.rs`.
- `commit_delivery` takes a `DeliveryCommit`; `apply_delivery`'s doc corrected; `follow_reported_model`
  split (`reported_model`); `LeasedTerminal` in its own module; one `Typed::answer` for both typing
  tools.
- Kept as is: `LeasedTerminal` polls `read_terminal` (the port has no offset-only read; ~50 ms
  normally, 5 s at most); the per-owner guard map is not pruned (one small entry per orchestrator);
  `paste_and_submit_to`'s `echo_seen.unwrap_or(false)` is unreachable (text is never empty there).

## Verification

- `tddy-tool-engine/tests/terminal_typing.rs` — a fake TUI that reads in batches (writes merge) and
  holds what trails `ESC[201~`; `tddy-coding-agent-specs/tests/spec_model_report.rs`;
  `tddy-session-activity/tests/model_report_record.rs` and `session_watch_wake_and_pick_unit.rs`;
  `echo_seen` pinned in `tddy-daemon-rpc`'s turn-control and terminal-keys acceptance, whose
  stand-in agents echo what they are typed, as a TUI does; `project_home_watch_wake_acceptance`.
- Live, through `tddy-session-driver`'s `send_keys` (one call, text + Enter) against Cursor Agent
  2026.11.22: local pty, local tmux, `coder ssh` → remote tmux — all submitted, echo seen after
  50 / 60 / 120 ms.

## Limits

- A `usage` source reads the transcript on the daemon's host: a Claude session on a remote target,
  whose transcript lives there, records the `SessionStart` model only.
- Recording a Claude alias's resolved id (`opus` → `claude-opus-5-5`) pins the resume to that id.

## Deferred (operator decision 2026-11-24)

Two backlog entries, kept in `docs/dev/todo/`:

- `2026-11-24-cursor-reply-text-not-in-transcript.md` — a Cursor session's replies never reach its
  ACP transcript (Claude's event names hardcoded, `afterAgentResponse` not installed, status-less
  reports dropped, Cursor's chat id not counted as the session's own conversation).
- `2026-11-24-cursor-model-list-lacks-tui-models.md` — `agent --list-models` omits models the TUI's
  `/model` picker offers (GLM 5.3 / 5.3 Flash); Cursor's gap.

## Docs

- [project-concept.md § Typing into a terminal](../../ft/daemon/project-concept.md#typing-into-a-terminal),
  [§ Sending a session a turn](../../ft/daemon/project-concept.md#sending-a-session-a-turn),
  [§ Watching sessions](../../ft/daemon/project-concept.md#watching-sessions)
- [cursor-cli-session.md](../../ft/daemon/cursor-cli-session.md),
  [claude-cli-session.md](../../ft/daemon/claude-cli-session.md)
- [tddy-tool-engine terminal-typing.md](../../../packages/tddy-tool-engine/docs/terminal-typing.md)
- [tddy-coding-agent-specs § Model report](../../../packages/tddy-coding-agent-specs/docs/coding-agent-specs.md#model-report-coding_agentmodel_report)
- [tddy-session-activity activity-service.md § Following the reported model](../../../packages/tddy-session-activity/docs/activity-service.md#following-the-reported-model),
  [session-notifications.md § Session watches](../../../packages/tddy-session-activity/docs/session-notifications.md#session-watches-session_watches)
- [tddy-daemon-rpc architecture.md § The watch waker](../../../packages/tddy-daemon-rpc/docs/architecture.md#the-watch-waker-exec_toolwatch_wakers)
- [tddy-session-driver architecture.md](../../../packages/tddy-session-driver/docs/architecture.md)
