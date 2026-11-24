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
# cursor.yaml — the main conversation's own events, never a (subagent's) tool call
model_report:
  - { kind: hook_field, field: model, events: [sessionStart, beforeSubmitPrompt, stop] }
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
- Cursor's `auto` is recorded as reported (`auto-smart`) — operator decision 2026-11-24;
  `agent -p … --model auto-smart` was verified to run.
- The record is read again just before the model is written, after the transcript read's await, so a
  status or rotation written meanwhile is kept.

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
- `plan_delivery` returns a `PlannedDelivery { delivery, commit: DeliveryCommit }`;
  `commit_delivery` applies the `DeliveryCommit`.
- **The waker wakes each orchestrator in a task of its own** (`watch_wake.rs`
  `run_watch_waker`): one that never echoes no longer delays another's wake by its 5 s, nor holds up
  the cue loop. An orchestrator is woken by one wake at a time — one coming due while its last is in
  flight fires when that one ends, still inside the minimum gap.
- Pinned by `session_watch_wake_and_pick_unit` (watch operations complete while a wake's paste is
  held open; neither a collection nor a stream call during the paste is handed the pasted record; a
  watch cancelled during the paste stays cancelled) and `project_home_watch_wake_acceptance`
  (`an_orchestrator_that_never_echoes_does_not_hold_up_anothers_wake`: 5.1 s late on the serial
  waker, prompt now).

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

## Validation Results

Run 2026-11-24 against `git diff origin/master...HEAD` (3 commits; ordinary branch, not a stack).
Review only: no code or test file was changed.

### /validate-changes

- **Build** (scoped): `./dev cargo build -p tddy-tool-engine -p tddy-session-driver
  -p tddy-coding-agent-specs -p tddy-session-activity -p tddy-daemon-rpc` — ✅ clean, no warnings.
- **Changeset sync**: every State B item has code behind it — ✅ `encode_typing`/`type_into`/`EchoWait`
  (`terminal_keys.rs`), ✅ three `TypingTerminal` adapters, ✅ `TypingError`, ✅ `echo_seen` in the three
  answers, ✅ `model_report` slot + load-time refusals, ✅ `follow_reported_model`, ✅ wake plan/commit
  outside the project lock + per-owner delivery guard. No unplanned code found.
- **Risks**
  - ⚠️ `tddy-daemon-rpc/src/exec_tool/watch_wake.rs:197-245` — the watch waker is **one serial task
    for every owner on the host**; `try_wake` now awaits the echo wait (0.5 s settle cap, up to 5 s
    with no echo), so one non-echoing orchestrator delays every other owner's wake by up to 5 s, and
    cues arriving meanwhile can be dropped when `WAKE_CUE_CAPACITY` fills (`try_send`). The lock
    change fixed the store side, not the waker loop.
  - ⚠️ `tddy-session-activity/src/service/model_report.rs:29-63` — read-modify-write of
    `.session.yaml` with an `.await` (transcript parse in `spawn_blocking`) between the read and the
    `..meta` write: a hook report landing meanwhile (status, rotation) is overwritten with the stale
    copy. Only when the model changed, so rare; re-read the record just before writing the model (or
    add a field-level `update_model` beside `update_activity_status`).
  - ℹ️ `tddy-tool-engine/src/project_home_terminal_tools/leased_terminal.rs:21-27` — each 10 ms poll
    renders a full `TextGrid` under the capture lock (the same lock the PTY reader appends under);
    `LiveTerminal` reads `end_offset()` alone. A port method for the offset only would be cheaper.
  - ℹ️ `tddy-session-activity/src/session_watches/delivery.rs:110` — `apply_delivery`'s doc still
    says `deliver_wake_with` "rolls `state` back when its delivery is refused"; it now plans on a copy.
  - ℹ️ `tddy-session-activity/src/session_watches.rs:147` — `deliveries` keeps one guard per owner
    ever seen, never pruned (small, unbounded).
  - ℹ️ `tddy-daemon-rpc/src/exec_tool/project_home/turn.rs:234` — `echo_seen.unwrap_or(false)` reports
    "no echo" for a typing that waited on nothing; unreachable today (text is never empty there).
  - ℹ️ Cursor `model_report` applies to **every** hook event: if Cursor's subagent hooks carry the
    subagent's `model`, the record would flip to it. Not covered by the live evidence above.
  - ℹ️ `auto-smart` is recorded and relaunched as `--model auto-smart`; the evidence shows
    `glm-5p3-flash-high` accepted by `--model`, not `auto-smart`.

### /validate-tests

Tests analyzed: 7 new (`terminal_typing.rs`), 8 new (`spec_model_report_acceptance.rs`), 7 new
(`model_report_record.rs`), 2 new (`session_watch_wake_and_pick_unit.rs`), plus assertion updates in
`project_home_turn_tools.rs` and 7 `tddy-daemon-rpc` acceptance files. Critical: 0. Warning: 3.

- ⚠️ Naming: new file `tddy-coding-agent-specs/tests/spec_model_report_acceptance.rs` uses the
  `acceptance` suffix fluent-tests rejects — `spec_model_report.rs`.
- ⚠️ Missing coverage in `terminal_typing.rs`: the `settle_cap` path (screen never holds still),
  `TypingError::NothingSent` / `KeysNotSent` (a write or an offset read that fails), and
  `encode_typing` in raw (non-bracketed) mode with a multi-line text (the split length is recomputed
  through `text_bytes`).
- ⚠️ Missing coverage in `tddy-session-activity`: the stream path's delivery guard
  (`take_stream_batch_picking` during a wake) and `commit_delivery`'s "never back" / "cancelled
  meanwhile" rules have no test.
- ℹ️ `model_report_record.rs` builds a 35-field `SessionMetadata` literal (Giant Constructor) — same
  copy as six sibling test files; a shared builder in a test-support module would serve all.
- ℹ️ `spec_model_report_acceptance.rs` refusal tests assert `contains("model_report") &&
  contains("usage")` — prefer the exact refusal text.
- ℹ️ The recording-agent shell script is duplicated across 6 `tddy-daemon-rpc` test files (each
  edited here); pre-existing duplication.

### /validate-prod-ready

Status: ✅ Ready. Blockers: 0. Mock code 0, unconsented fallbacks 0 (the 5 s no-echo
"press anyway" is operator-approved 2026-11-24, commented at `terminal_keys.rs:244-252` and logged
at `warn`), TODO/FIXME 0, debug output 0, `println!`/`eprintln!` 0. `tokio` `test-util` is added to
`[dev-dependencies]` only. `Typing::is_empty` is unused but required beside a `pub fn len`
(`clippy::len_without_is_empty`).

### /analyze-clean-code

**Score: C** (1 must-refactor, 3 needs-attention).

- 🔴 Must refactor — `tddy-session-activity/src/session_watches/delivery.rs` `commit_delivery`: 6
  parameters (mirrors `apply_delivery`'s 6). Have planning return a `PlannedDelivery { delivery,
  positions: Vec<(watch_id, u64)>, handed_endings }` and commit that, which also drops the whole-state
  `clone()` in `deliver_wake_with`.
- ⚠️ `store.rs` `deliver_wake_with` ~56 lines; `terminal_keys.rs` `type_into` 47 lines;
  `service/model_report.rs` `follow_reported_model` 42 lines (extract the source loop into
  `reported_model(&spec, &req, …) -> Option<String>`).
- ℹ️ `terminal_keys.rs:161` `impl Typed` precedes `struct Typed` (`:194`); `project_home_terminal_tools.rs:33`
  `mod leased_terminal;` sits between `use` lines; the `answer["echo_seen"] = json!(…)` patch
  repeats 3× (`Typed::answer`, `send_session_message`, `resume_session`).
- Oversized files (all pre-existing, grown ≤16 lines here): `slots/mod.rs` 695 (+1, recorded in
  `code-issues/complexity-slots-mod-render.md`), `spec.rs` 667 (+16), `watch_wake.rs` 543 (+1) —
  split later.

### Review findings resolved (2026-11-24)

- ✅ Serial waker → one task per wake, with a test (see State B).
- ✅ `.session.yaml` read-modify-write across an await → re-read just before writing.
- ✅ Cursor `model_report` limited to `sessionStart`, `beforeSubmitPrompt`, `stop`; test that a
  tool call on another model does not re-point the session.
- ✅ Tests added: settle cap, `TypingError::NothingSent` / `KeysNotSent`, raw multi-line
  `encode_typing`, stream call during a wake, watch cancelled during a wake.
- ✅ `spec_model_report_acceptance.rs` → `spec_model_report.rs`.
- ✅ `commit_delivery` takes a `DeliveryCommit` (5 parameters); `apply_delivery`'s doc corrected;
  `follow_reported_model` split (`reported_model`); `impl Typed` after `struct Typed`.
- Kept as is, with reasons: `LeasedTerminal` polls `read_terminal` (the port has no offset-only
  read; the poll lasts ~50 ms normally, 5 s at most); the per-owner guard map is not pruned (one
  small entry per orchestrator); `turn.rs` `unwrap_or(false)` is unreachable (text is never empty);
  the 35-field `SessionMetadata` literal and the duplicated stand-in script are pre-existing test
  patterns shared with sibling files.

## Refactoring Needed

### From /validate-changes
- [ ] Waker loop: run each due owner's `try_wake` concurrently (or spawn per owner), so one
  non-echoing orchestrator does not stall the others (`watch_wake.rs:229-244`).
- [ ] `follow_reported_model`: re-read `.session.yaml` after the transcript parse, before writing.
- [ ] Fix `apply_delivery`'s stale doc (`delivery.rs:110`).

### From /validate-tests
- [ ] Rename `spec_model_report_acceptance.rs` → `spec_model_report.rs`.
- [ ] Add `type_into` tests: settle cap, write/offset failures, raw-mode multi-line split.
- [ ] Add store tests: stream call waits on a wake's guard; commit never moves a position back and
  skips a watch cancelled meanwhile.

### From /validate-prod-ready
- None.

### From /analyze-clean-code
- [ ] `commit_delivery` → take a `PlannedDelivery` (≤4 parameters).
- [ ] Shorten `deliver_wake_with`, `type_into`, `follow_reported_model` under 40 lines.
- [ ] Move `impl Typed` below `struct Typed`; move `mod leased_terminal;` out of the `use` block.
