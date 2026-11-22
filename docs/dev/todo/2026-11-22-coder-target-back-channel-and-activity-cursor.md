# 2026-11-22 — A Coder-target child's reports never reach its orchestrator; activity paging, the start's branch and the target terminal's colours are broken

**Category:** Defect (orchestrator ↔ child communication on targets, and the orchestrator's follow tools)
**Source:** live probe on 2026-11-22. Orchestrator `01a16dc6-ab4c-7dc3-844e-baca4514f86e` started
claude-cli child `01a201cd-1cd1-7c53-8481-77991a117fac` on the derived Coder host `remote-dev`
(worktree `/home/coder/.tddy/repos/tddy-coder/.worktrees/01a201cd-…`, base `0a76c98fb`). A local
claude-cli probe `01a201d5-0acf-7100-9d28-537e6eee952b` was the control for gaps 4–6. Evidence was
read from the installed Tddy Desktop daemon (built 2026-11-22 07:42, carries `#orchestra` 2/9), its
log `~/.tddy/logs/daemon`, the session's directory, the project log
`~/.tddy/projects/51777f55-…/notifications.jsonl`, and the remote worktree over `coder ssh`.
Branch: `investigate/coder-target-back-channel`.

Each defect below names its root cause, the failing test that pins it, the fix and the blast radius.

**Status (2026-11-22, same branch):** fixed — 1a, 1b, 1c, 2a (as decided: `log_seq`, below), 2b, 3a,
3b, 5, 6 and 7. Gap 4 closed as by-design. Still open, as TODOs: the live re-run under *What would
close this file*; and the stack-child changeset line
(`stack_doc_attachments::attached_changeset_prompt_line`) still names its file
`artifacts/attachments/<name>`, relative to the session directory rather than the agent's working
directory — gap 5's line now names every attachment by a readable path beside it, but that older
line was left as it is (TODO: point it at the same path, or drop it in favour of gap 5's line).

## Defect 1 — a target child's `TddyMessage` reports are lost

### What happened

The child ran its guidance's command twice, from its worktree:

```bash
echo '{"text": "Step 1 done: …"}' | printf '%s\t%s\t%s\n' 'TddyMessage' '680361a1-…' "$(cat | tr -d '\n')" >> '.tddy/hooks.log'
```

The remote `.tddy/hooks.log` holds **both lines, each with an empty payload** (lines 8 and 14 of
17): `TddyMessage\t680361a1-…\t`. Every hook line around them (`PreToolUse`, `PostToolUse`) has
its JSON. The project log holds no `message` record from the child, and the daemon log has neither
`a message line without text was dropped` nor `its report to its orchestrator was refused`.

### 1a. Root cause — the guidance command needs a shell that is not zsh

`remote-dev`'s login shell is `/usr/bin/zsh`, and Claude Code's Bash tool runs commands in it. zsh
runs the **last element of a pipeline in the current shell**, so `"$(cat | tr -d '\n')"` — an
argument of `printf`, the last element — is expanded before stdin is the pipe, and reads nothing.
bash and dash fork the last element first, so the same line works there:

```text
$ zsh  -c "echo '{\"text\": \"x\"}' | printf '%s\t%s\n' TddyMessage \"\$(cat | tr -d '\n')\""   →  TddyMessage\t
$ bash -c …                                                                                       →  TddyMessage\t{"text": "x"}
```

The hooks themselves are unaffected: Claude Code feeds a hook its payload as the **command's own
stdin**, with no pipeline in front of it, so `remote_hook_command` works for them in any shell.

- `packages/tddy-agent-backend/src/remote_hook_command.rs:34` — the command, `printf … "$(cat | tr -d '\n')" >> <log>`.
- `packages/tddy-agent-launch/src/back_channel.rs:136` — the guidance puts `echo '{…}' |` in front
  of that same command (`hook_log_channel`, `back_channel.rs:106`).

### 1b. Root cause — a dropped line is invisible

`HookLine::parse` refuses the empty payload as `BrokenPayload`, and the follower only increments a
counter nobody reads: `packages/tddy-agent-launch/src/remote_hook_follower.rs:381`
(`deliver_whole_lines`). Nothing is logged, so the loss looked like a delivery bug.

### 1c. Contributing — `ReadSessionMessages`' `lost` is not what it says

`InboxRead::lost` is documented as "messages addressed to the reader that the log evicted"
(`packages/tddy-session-activity/src/session_messages.rs:125`), but `read_inbox`
(`session_watches/store.rs:303`) returns `records_after`'s count, which is **every evicted record
of the project** after the reader's inbox position (`project_notification_log.rs:319`,
`lost_after` at `:419`). The orchestrator had never read its inbox (position 0), the project's
window is 1,000 records and its newest seq was ~1,934, so `lost` was 934, then 935 after one more
activity record — a number with nothing to do with the orchestrator's messages.

### Local vs target

A local child does **not** use this route: it reports over native peer messaging or
`tddy-tools send` (`back_channel_for`, `back_channel.rs:88`), and those reached the orchestrator in
this investigation. The loss is specific to the hook-log route, so it is **target-only** (Coder and
SSH alike) and **shell-dependent**: any target whose login shell is zsh — including every macOS SSH
host, where zsh is the default — loses every report.

### Pinned by

- `packages/tddy-agent-launch/tests/target_back_channel_shell_acceptance.rs` —
  `a_report_run_from_a_zsh_login_shell_is_a_message_line_the_follower_accepts`: runs the exact
  guidance line under `zsh -c` in a stand-in worktree and parses the log line. Needs `zsh` on
  `PATH`; it fails naming zsh when absent (see *Open decisions*).
- `packages/tddy-session-activity/tests/session_messages_unit.rs` —
  `a_read_counts_as_lost_only_evicted_messages_addressed_to_the_reader`.

### Proposed fix

1. Give the child a command that does not read a pipe inside a command substitution — e.g. the
   report as `printf`'s own argument: `printf '%s\t%s\t%s\n' 'TddyMessage' '<token>' '{"text": "…"}' >> '.tddy/hooks.log'`
   (the guidance then says the JSON must be on one line and single-quote-safe), or run the existing
   command under an explicit POSIX shell: `echo '{…}' | sh -c '<command>'`. No fallback either way.
2. `deliver_whole_lines` logs every dropped line with its reason (`HookLineError`'s `Display`) and
   the session id — not its payload, which is untrusted.
3. `lost` counts only evicted records that were messages addressed to the reader — or, if the log
   cannot know that once a record is gone, the field is renamed and documented as the project-wide
   eviction count it is. Either way, a reader that has never read must not be told hundreds were
   lost.

**Blast radius:** (1) `back_channel.rs` and its acceptance test's expected guidance text; resumed
sessions re-render guidance (`cli_agent_steps/resumed_wiring.rs:107`), so a resumed target child
picks the new command up. Hook commands are untouched. (2) one log line in the follower. (3)
`read_inbox` and the `ReadSessionMessages` answer; the web does not read `lost`.

## Defect 2 — `ReadSessionActivity` cannot page incrementally

### 2a. Root cause — persisted rows never carry an `activity_seq`

`page_activity` (`packages/tddy-tool-engine/src/session_follow.rs:124`) pages by
`activity_seq`: it keeps `r.activity_seq > cursor` (`:134`) and answers the last item's seq as
`next_cursor` (`:142`). But every row `ReportAgentActivity` appends carries `activity_seq: NO_TICK`
(= 0) — `packages/tddy-session-activity/src/service/hook_reports.rs:306` (`PreToolUse`) and `:339`
(`PostToolUse`) — and nothing writes a seq back to `agent-activity.jsonl`. The only stamping is the
LiveKit session room's `tick_activity` (`packages/tddy-daemon-livekit/src/session_room.rs:2103`),
**in memory, on the broadcast copy**, where `activity_seq` means the *tick's delta seq*, shared by
every call that tick covered.

So the first page is everything (no cursor), `next_cursor` is `"0"`, and a poll from `"0"` filters
out every row: `items: []` forever. On this host **0 of 38,290** persisted rows across all sessions
has a non-zero seq. This is **general**, not target-specific; a target session merely also has no
room (its worktree is not on the daemon's host — `session_reach` logged so for the probe).

The existing paging tests (`packages/tddy-tool-engine/tests/session_follow_paging.rs`) build rows
with unique seqs 1..n, a shape production never writes.

### 2b. Root cause — coalescing drops a call's start time

`read_all_agent_activity` (`packages/tddy-changeset/src/agent_activity.rs:275`) coalesces rows by
`call_id` by **replacing the whole record** (`:296`). The `PostToolUse` row has
`started_unix_ms: 0` (`hook_reports.rs:333`), so every finished call reads back with no start
time. Only calls with no completion row (the probe's `AskUserQuestion`, still waiting) kept theirs.
General; `read_agent_activity` (the web pane's capped tail) inherits it.

### Pinned by

- `session_follow_paging.rs` — `a_poll_from_the_last_cursor_returns_the_calls_hooks_recorded_since`,
  `successive_pages_of_hook_recorded_calls_advance_the_cursor` (rows as hooks persist them).
- `packages/tddy-changeset/tests/agent_activity_coalesced_times.rs` — whole and capped reads.

### Proposed fix

2a needs a design decision (see *Open decisions*): `activity_seq` already has a meaning (the
room's tick/delta seq, which `tddy-session-sync` de-duplicates by), so it should not be overloaded
with a per-row cursor. Options:

- **A. Page by position.** The cursor is the call's index in `read_all_agent_activity`'s
  first-seen order. Needs no storage change and works for existing logs. A call whose row is
  updated later (running → completed) is not re-delivered by a poll — the follower needs a way to
  see status changes (e.g. the page also returns calls still `running` before the cursor).
- **B. A persisted per-row sequence** (a new field, e.g. `log_seq`, assigned under the append's
  lock — the counter can be recovered from the file's line count on start). Re-delivers a call when
  its completion row lands. Existing rows have none, so a reader numbers them by line on read.

2b: coalesce field-wise — the later row supersedes status, result, error and completion, and keeps
the earlier non-zero `started_unix_ms` (and input when the later is null).

**Blast radius:** 2a — `ReadSessionActivity` only (A), or also `agent-activity.jsonl`'s schema and
its writer in `hook_reports.rs` plus `tddy-session-sync`/LiveKit readers that must ignore the new
field (B). 2b — every reader of the coalesced log (web Agent Activity pane, `ReadSessionActivity`);
a start time appearing where there was 0 is the only visible change.

## Defect 3 — `GetSessionStart` drops the branch once the start succeeds

### Root cause

- **3a. A target start never records its branch.** A local cut writes `changeset.branch`
  (`packages/tddy-session-worktree/src/worktree.rs:58` and siblings). The remote cut,
  `remote_worktree_through` (`packages/tddy-agent-launch/src/cli_session_worktree.rs:267`), only
  emits `worktree_ready` (`:340`). The probe's `changeset.yaml` has `workflow.new_branch_name` and
  no `branch:`. The session listing takes the branch from the changeset
  (`packages/tddy-session-activity/src/session_list_enrichment.rs:269`), so it lists `""`.
- **3b. An empty listed branch overwrites the reported one.** `apply_ending`
  (`packages/tddy-tool-engine/src/start_ledger.rs:404`) does `branch.or(record.branch.take())`;
  the listing's `Some("")` wins over the branch the `worktree_ready` event recorded while running.
  The worktree survives only because the target's listing does carry `repo_path`.

### Pinned by

- `packages/tddy-tool-engine/tests/start_ledger_target_branch.rs` —
  `a_succeeded_target_start_keeps_the_branch_it_cut_when_the_host_lists_none` (3b).
- 3a is **not pinned**: no suite drives a full target start (remote clone and cut through a fake
  shell); the Coder suites call `launch_on_target` directly. TODO: pin it with the fix, at
  `remote_worktree_through`'s seam (a recorded changeset after a fake `setup_remote_worktree`).

### Proposed fix

3a: after `setup_remote_worktree`, write `checkout.branch` into the session's changeset, as the
local cut does. 3b: treat an empty listed branch/worktree as absent
(`branch.filter(|b| !b.is_empty()).or(…)`). **Blast radius:** target starts' changesets (the PR
stack, `ListProjectSessions` and the web drawer then show the branch); the ledger's ending only.

## Gap 4 — a claude-cli child's shell has no `TDDY_SESSION_DIR`

Reported by the orchestrator from the local probe. **By design today**: `wire_session_agent`
(`packages/tddy-agent-launch/src/cli_agent_steps.rs:119`) keeps every `TDDY_*` variable a launch
produces on the agent's tool server, never the agent process; a shell-run `tddy-tools` finds its
session through the session pointer written into the worktree. `TDDY_SESSION_DIR` reaches a
process only through a managed workflow (`managed_launch.rs:78`) or the tddy-coder backends
(`tddy-agent-backend/src/backend/codex.rs:67`). On a target the session directory is on the
daemon's host and unreachable anyway.

**Closed as by-design (operator, 2026-11-22):** `TDDY_SESSION_DIR` is not exported to the agent.
`TDDY_*` stays off the agent process, and on a target the session directory is on the daemon's
host, where the agent cannot read it. What the child actually needed — the files it was given — is
gap 5's to deliver.

## Gap 5 — nothing tells a child its attachments exist or where they are

`StartSession` materializes attachments into `<session dir>/artifacts/attachments/` before the
agent starts (`packages/tddy-agent-launch/src/svc_start_session_core.rs:342`), but neither the
launch guidance nor the first prompt names them; only a stack child's spawn names its attached
changeset (`tddy-session-files/src/stack_doc_attachments.rs:117`, `prompt_with_attached_changeset`).
The probe's first move was `find /`. On a target the directory is on the daemon's host, so naming
it is not enough there: the files must be copied into the remote worktree (e.g. `.tddy/attachments/`,
git-excluded like the rest of `.tddy`) and that path named. **Not pinned yet** — the contract (which
prompt or guidance carries the list) is the fix's to choose; a red test written now would invent it.

## Gap 6 — a Coder-target session's terminal has no colours

`ReadSessionTerminal` with `styles: true` returned `styles: []` on every row for the Coder probe and
fg/bg/bold runs for the local one, so the remote agent emits no colour. Root cause: the target
command runs the agent under `exec env -i HOME="$HOME" PATH="$PATH" TMPDIR=…` plus only the spec's
`pass_env` and the launch's own variables (`packages/tddy-agent-launch/src/remote_agent_launch.rs:61`,
`:71`, `remote_command` at `:355`). The local PTY sets `TERM=xterm-256color` and
`COLORTERM=truecolor` on its child (`packages/tddy-pty/src/runtime.rs:79`), which on a target is
the `coder`/`ssh` client — its remote pty session may carry `TERM`, but `env -i` removes it before
`claude` starts. With no `TERM`, Claude Code renders monochrome.

**Pinned by** `remote_agent_launch.rs` unit test `the_agent_on_a_target_is_told_the_terminal_it_draws_on`.
**Fix:** set `TERM=xterm-256color COLORTERM=truecolor` explicitly inside the `env -i` — the terminal
the daemon's PTY emulates, as the local route names it; no inheritance from the remote login.
**Blast radius:** every agent launched on a target (main agent, agent tabs, one-shots through
`target_login_exec`); the exact-string test of the remote command changes.

## Gap 7 — an orchestrator's answer to an `AskUserQuestion` is accepted and then dropped

Seen on this investigation's own session (`01a201d3-2776-7133-86de-8d6cfbc56349`, local claude-cli,
answer wait 120 s): one `AskUserQuestion` call produced **two** pending questions. `c100748f-…` was
recorded at `1795327906168` by the waiting `PreToolUse` hook; nobody answered it within the 120 s
hold, so the hook printed nothing. At `1795328026661` — **120.49 s later**, when that hook returned —
`35121368-…` was recorded for the same call. The orchestrator then answered `c100748f` and was told
`no permission request 'c100748f-…' is pending on that session`. It answered `35121368` at
`1795328144828`, the answer was accepted, but Claude kept showing its own dialog until the answers
were typed into the terminal.

### Root cause

- **7a. Claude fires `PermissionRequest` for the question once `PreToolUse` lets it through.** That
  hook also waits (`--await-answer`, `PermissionRequest` matcher). `pending_question_record`
  (`packages/tddy-session-activity/src/pending_question_record.rs:41`) derives a record's kind from
  the **tool**, so the `PermissionRequest` of `AskUserQuestion` is recorded as a new `select`
  question, replacing the first in the session's single pending slot, and the daemon holds it as a
  question (`service/question_answers.rs`, `wait_for_question`).
- **7b. The answer is then mapped as a permission.** The hook chooses its decision by **event**:
  `tddy-tools` `session_hook.rs` sends every `PermissionRequest` to `await_decision` →
  `permission_decision_json` (`packages/tddy-session-activity/src/hook_decision.rs:22`), which maps
  only `allow_*`/`reject_*` option ids. A question's answer has none, so it returns `None`, nothing
  is printed, and Claude shows its dialog. `AnswerSessionQuestion` has already reported success.
  The target path has the same split: `HookLine::question_input`
  (`packages/tddy-agent-launch/src/remote_hook_answer.rs:56`) is set only for `PreToolUse`.
- **7c. The refusal names the wrong thing.** `AnswerRefusal::UnknownRequest`
  (`packages/tddy-session-activity/src/answers.rs:142`) always says "permission request", also for
  a question whose hook has stopped waiting.
- The Coder probe's answer arrived inside the first `PreToolUse` window, which is why it worked
  there.

### Fix

A `PermissionRequest` of a question tool hands the answers over as Claude's `PermissionRequest`
decision `{"behavior": "allow", "updatedInput": <tool_input with answers>}` — the same
`updatedInput` the `PreToolUse` decision carries — on both the local hook and the target's answer
writer. `UnknownRequest` says "question" or "request" neutrally. **Blast radius:** the hook's decision
for questions only; permissions are unchanged.

### `other` beside a single-choice selection — intended

`validate_question_answers` (`packages/tddy-session-activity/src/answers/question.rs:118`) refuses a
single-choice answer with both a label and `other` ("give one answer, not 2"). This is intended: it
is Claude's single-choice dialog, where "Other" *replaces* a choice, and Claude's answer for a
single-choice question is one string. An orchestrator that wants to add a note picks `other` alone
and writes the choice into it, or sends the note as a session message.

## Decisions (operator, 2026-11-22)

- **Defect 2a**: B — a persisted per-row sequence beside `activity_seq` (which keeps its
  tick/delta meaning). Built as `SequencedActivity::log_seq` (`tddy-changeset`): the 1-based line of
  a call's latest row in the append-only `agent-activity.jsonl`. The log persists it — no counter to
  keep or recover, no race between concurrent appends, and existing rows are numbered with no
  migration — and a call's completion row moves it forward, so a poll hands it over again finished.
  `ReadSessionActivity` items carry `log_seq`; `next_cursor` is the last one's.
- **zsh**: declared in the dev shell (`flake.nix`, beside `pkgs.tmux`) so CI runs the defect-1 test.
- **Gap 4**: by-design (above).
- Fixes for 1, 2, 3, 5 and 6 land on this branch.

## What would close this file

Each defect's test green, plus a live re-run of the probe on `remote-dev`: two reports arrive in the
orchestrator's `ReadSessionMessages` with `lost: 0`, a `ReadSessionActivity` poll from the last
cursor returns only the new calls with their start times, `GetSessionStart` keeps the branch after
`succeeded`, and the terminal carries colour runs.
