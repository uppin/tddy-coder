# 2026-11-25 — Transcript activity follow-ups: hold before registration, poll the transcript, order around lost hooks

**Type:** Fix
**Packages:** `tddy-agent-launch`, `tddy-session-activity`, `tddy-daemon-kernel`, `tddy-session-lifecycle`, `tddy-session-driver`
**PR:** #1037

## Problem

Live probe of #1035 (Cursor session `01a21114-abc1-78c2-984d-b5f83ea9a596` on Coder
`mantasi/remote-dev`, under tmux, from the Mac daemon) found three defects in the transcript
activity source:

- **n1: hook lines lost before registration.** The hook follower started at 07:03:36. Every
  first-turn hook (beforeSubmitPrompt, pre/postToolUse ×3, stop) was refused at 07:03:41 with
  `session not found`. The session registered at 07:03:42, and the follower advanced past the
  refused lines, so turn 1's tool calls are lost for good.
- **n2: transcript replayed on every new prompt.** `ReadSessionTranscript` shows turn 1 three
  times and turn 2 twice. Cursor rewrites its transcript atomically on every new prompt (a temp
  file renamed over it), and `tail -F` follows the new file from its first byte.
- **n3: text before a tool call sorts after it.** In turn 3 the agent's opening sentence landed
  after the Grep and Shell calls. Turn 1's transcript tools (hooks lost to n1) stay unpaired —
  Cursor's file carries almost no `turn_ended` lines (one in 142) — and pair with turn 3's hooks
  of the same tools, consuming them before turn 3's transcript arrives, leaving its opening text
  with no unreached hook to be stamped before.

## Change

### n1: hold and retry until the session exists

`HookReportDelivery` now answers `HookDeliveryError` (code + message, defined in
`tddy-agent-launch` so implementors need no new dependency) instead of a bare string, preserving
the activity service's `Status`. `ActivityHookSink` and the transcript sink hold a report refused
only with `NOT_FOUND` + `SESSION_NOT_REGISTERED_YET` (`tddy-session-activity`, the message
`hook_session` answers when `.session.yaml` is not on disk yet) and retry with backoff (100 ms,
doubling to 1 s) until served, refused for another reason, or aborted with the follower. Any
other refusal is consumed as before. Decision: hold-and-retry, not start-after-registration —
it fixes the root cause (a refused line treated as consumed) on every launch path (remote, host
PTY, host tmux) without introducing exit-hook races for an agent that exits at once.

### n2: poll the transcript instead of tailing it

`follow_transcript` uses `FileFollower::start_polling`: every 500 ms one `wc -c` asks the size,
grown one `tail -c` reads only the new suffix, whole lines delivered, the offset saved past
each. A rewrite with a stable prefix delivers only its new suffix; a file that does not exist
yet reads as empty until its first prompt. One-shot commands exit at once, so `alive_while` is
not watched. The hook log stays streamed (`tail -f`, append-only, latency-sensitive).

### n3: a hooks turn end drops unpaired transcript calls

`ToolTurns::hooks_turn_ended` now drops unpaired transcript calls with the turn (the hooks'
own stay for a behind follower). A `Done` means the turn's hooks all fired, so anything the
transcript reported that no hook paired with belongs to an earlier turn whose hooks never
arrived or to a re-read line — left in place it would pair with the next turn's hooks and
misorder its opening text. Safe live (paired already, clearing is a no-op) and behind (the
current turn's transcript has not arrived yet; only previous leftovers clear).

## Acceptance tests

| Test | Proves |
|---|---|
| `remote_hook_follower_acceptance::hooks_logged_before_the_session_is_registered_land_once_it_is_with_no_duplicates` | n1: hooks before registration land once it exists, no duplicates |
| `transcript_follower_acceptance::a_new_prompt_a_reattach_and_a_resume_each_append_only_new_lines` | n2: atomic rewrite, reattach and resume append only new lines |
| `cursor_transcript_activity::in_a_third_turn_behind_the_hooks_the_opening_text_still_comes_before_its_tools` | n3 behind: prompt, opening, Grep, Shell, closing |
| `cursor_transcript_activity::in_a_third_turn_live_the_opening_text_still_comes_before_its_tools` | n3 live: same order, transcript line before hook rows |
| `cursor_transcript_activity::with_first_turn_hooks_lost_the_third_opening_still_comes_before_its_tools` | n3 probe shape: lost first-turn hooks, no per-turn `turn_ended` |
| `cursor_transcript_activity::with_duplicate_earlier_turns_the_third_opening_still_comes_before_its_tools` | n3 robust to n2-style duplicates |

## Validation Results

- Scoped `cargo clippy -p tddy-agent-launch -p tddy-session-activity -p tddy-daemon-kernel -p tddy-session-lifecycle -p tddy-session-driver --all-targets -- -D warnings`: clean.
- `./test -p tddy-daemon-kernel`: green.
- `./test -p tddy-session-activity`: green (8/8 cursor transcript incl. 4 new n3).
- `./test -p tddy-agent-launch`: green (n1 + 4/4 transcript incl. new n2).
- `./test -p tddy-session-lifecycle -p tddy-session-driver`: green.
- File length gate: no touched non-test source file crosses 500 production lines (measured at wrap).
- Live check: no Coder target or Cursor CLI on this host; exact steps in the PR description instead.

## Live-check steps (for the operator)

1. From the Mac daemon, start a Cursor session on a Coder target with `run_in_tmux`.
2. Run 3 turns, each prompting then two tools (e.g. Grep, Shell), waiting for the turn to end.
3. `ReadSessionTranscript`: every turn reads prompt, opening text, its tools, closing text, once —
   turn 1's tools present (n1), no turn repeated (n2), turn 3's opening before its tools (n3).
