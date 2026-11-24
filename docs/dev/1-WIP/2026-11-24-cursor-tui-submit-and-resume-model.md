# Changeset: cursor-tui-submit-and-resume-model

PRD (target): [`docs/ft/daemon/project-concept.md`](../../ft/daemon/project-concept.md) § Sending a
session a turn, § Typing into a terminal; [`docs/ft/daemon/cursor-cli-session.md`](../../ft/daemon/cursor-cli-session.md)
Branch: `fix/cursor-tui-submit-and-resume-model`

## Problem

Observed 2026-11-24, Cursor Agent 2026.11.19 in tmux on a Coder workspace target (and locally):

1. **Text + Enter sent together is not submitted.** `SendSessionKeys(text, keys: [Enter])`,
   `SendSessionMessage` and `ResumeSession(message)` left the text sitting in Cursor's input box; a
   second, separate Enter submitted it. Claude Code submits the identical bytes.
2. **A model picked inside the TUI is lost on resume.** `/model` → GLM 5.3 Flash, then a resume
   relaunched the agent with the start's `--model auto`.

### Root cause 1 — bytes trailing a paste end wait for the next input

Every delivery path encoded the text and the submit key into **one buffer and one write**
(`encode_keys` → `send_input`): `ESC[200~ text ESC[201~ \r`. Cursor's input handler (bundled
`5548.index.js`) inserts the paste at `ESC[201~` and stores the **rest of that read**
(`xt.current = remainder`), prepending it to the *next* input event — so the `\r` waits for another
key. Unbracketed, a multi-byte read is a paste burst held for 75 ms (`T=75`), and a `\r` inside the
window joins it as a newline.

Live matrix (pty harness writing exact chunks; SUBMITTED / LEFT in the box):

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

### Root cause 2 — the record's model is written once, at start

Every resume path renders `--model` from `.session.yaml` `model`
(`opencode_cli_spawn/resume.rs`, `svc_resume_sandboxed_cli_harness_session.rs`), and nothing updated
it after the start. Dropping `--model` on resume is no fix: both agents' `/model` rewrites a
**global** default (`~/.cursor/cli-config.json`, `~/.claude/settings.json`), not the session's.
Every Cursor hook event carries the session's current `model` (`glm-5p3-flash-high` after the pick;
`agent --model glm-5p3-flash-high` accepts it; `auto` reports itself as `auto-smart`), and it already
reached `ReportSessionStatus.hook_fields` — ignored. Claude Code's hooks name the model on
`SessionStart` only; its transcript names each assistant message's `message.model`.

## State B

### Typing waits for the echo (`tddy-tool-engine::terminal_keys`)

- `encode_typing` keeps the text and the keys as **two writes** (`Typing { text, keys }`); joined,
  they are exactly `encode_keys`'s bytes.
- `type_into(terminal, typing, &EchoWait)` writes the text, waits until the application has echoed
  it — the terminal's output offset moves past where it stood before the write, then holds still for
  30 ms (capped at 500 ms after the echo began, for a screen that never holds still) — and only then
  writes the keys. Text alone or keys alone is one write, with nothing waited on.
- **No echo within 5 s: the keys are pressed anyway** — deliberate, operator-approved
  (2026-11-24) — with a `warn` log and `echo_seen: false` in the answer, so the caller can tell.
- `TypingTerminal` is the port: an output offset and a writer. Three adapters:
  `LeasedTerminal` (`SendSessionKeys`, through `ProjectHomeTerminalPort`), `LiveTerminal`
  (`tddy-daemon-rpc`'s `paste_and_submit_to`: `SendSessionMessage`, `ResumeSession`'s first message,
  the watch waker; offset is `TerminalCapture::end_offset`) and `DriverSession`
  (`tddy-session-driver`'s `send_keys`).
- `TypingError` says how far typing got: `NothingSent` or `KeysNotSent` (the text was typed).

Answers:

| Tool | Before | After |
|---|---|---|
| `SendSessionKeys` | `{sent_bytes}` | `{sent_bytes, echo_seen?}` — `echo_seen` when keys followed text |
| `SendSessionMessage` | `{session_id, delivered_via}` | `+ echo_seen` for `terminal_paste` |
| `ResumeSession` | `{session_id, outcome}` | `+ echo_seen` when a first message was pasted |

`TurnDelivery::TerminalPaste { echo_seen }`, `ResumeOutcome::Resumed { message: Option<TurnDelivery> }`.

### The record follows the reported model (`coding_agent.model_report`)

New spec slot, tried in order on every hook report; the first source that applies to the event and
yields a model wins, and a model differing from `.session.yaml` `model` replaces it:

```yaml
# cursor.yaml
model_report:
  - { kind: hook_field, field: model }
# claude.yaml
model_report:
  - { kind: hook_field, field: model, events: [SessionStart] }
  - { kind: usage, events: [Stop], ignore: ["<synthetic>"] }
```

- `hook_field` reads a top-level string of the hook event (`hook_fields`); `usage` evaluates the
  agent's `usage` binding with `tddy-usage` over the session's current main conversation (the last of
  `agent_conversations`, else the usage context's) with **no fallback model**, so a transcript that
  names none yields none. `ignore` lists values that are not a model.
- Refused at load: a `hook_field` source with no field; a `usage` source on an agent without a
  `usage` binding.
- `ActivityServiceImpl::follow_reported_model` runs after `follow_conversation_rotation` in
  `ReportSessionStatus`. Resume code is unchanged: it already reads the record.
- Cursor's `auto` is recorded as reported (`auto-smart`) — operator decision 2026-11-24.

### A wake no longer holds the project's watch state while it pastes (`tddy-session-activity`)

The watch waker pastes through the same echo wait, which can take up to 5 s for an agent that
never echoes. Before, `SessionWatchStore::deliver_wake_with` ran its delivery under the project's
watch-state lock — which would have blocked every create, list, renew, collection and stream call of
the whole project for that long, once per wake in a row. Now:

- The delivery is **planned** under the lock on a copy of the state (`apply_delivery`), the lock is
  dropped, the paste runs, and only a successful paste is **committed** under the lock again
  (`commit_delivery`: each of the owner's live watches advances to its planned position, never back;
  the handed endings are taken; a watch cancelled meanwhile is simply gone). A refused paste commits
  nothing — there is nothing to roll back.
- **Per-owner delivery guard** (`SessionWatchStore::deliveries`): every hand-over to an owner —
  the wake (for its whole paste), `take_delivery`, `take_stream_batch_picking` — holds it, so what a
  wake is still pasting is never handed to the same owner twice. Only that owner's own hand-overs
  wait; a stream call holds it for its one locked step, never across its long poll.
- Pinned by `session_watch_wake_and_pick_unit`: watch operations complete while a wake's paste is
  held open, and an orchestrator collecting during the paste is not handed the pasted record again.

## Trade-offs and limits

- A `usage` source reads the transcript on this host: a Claude session on a remote target, whose
  transcript lives there, records the `SessionStart` model only.
- Recording a Claude alias's resolved id (`opus` → `claude-opus-5-5`) pins the resume to that id.

## Deferred (operator decision 2026-11-24)

- [2026-11-24-cursor-reply-text-not-in-transcript.md](../todo/2026-11-24-cursor-reply-text-not-in-transcript.md)
- [2026-11-24-cursor-model-list-lacks-tui-models.md](../todo/2026-11-24-cursor-model-list-lacks-tui-models.md)

## Verification

- Red → green: `tddy-tool-engine/tests/terminal_typing.rs` (a fake TUI that reads in batches —
  writes merge — and holds what trails `ESC[201~`), `tddy-coding-agent-specs/tests/spec_model_report_acceptance.rs`,
  `tddy-session-activity/tests/model_report_record.rs`, and `echo_seen` pinned in
  `tddy-daemon-rpc`'s turn-control and terminal-keys acceptance (stand-in agents now echo what they
  are typed, as a TUI does).
- Live, through `tddy-session-driver`'s `send_keys` (one call, text + Enter) against Cursor
  2026.11.22: local pty, local tmux, `coder ssh` → `remote-dev` tmux — all SUBMITTED, echo seen
  after 50 / 60 / 120 ms.
