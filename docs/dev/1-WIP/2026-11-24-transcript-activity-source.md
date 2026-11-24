# Activity from an agent's own transcript, followed live on the host and on targets

**Type:** Feature + Fix
**Packages:** `tddy-coding-agent-specs`, `tddy-usage`, `tddy-daemon-kernel`, `tddy-session-activity`,
`tddy-agent-launch`, `tddy-session-lifecycle`

## Problem

A session's activity feed (`ReadSessionActivity`, `ReadSessionTranscript`, the web Agent Activity
panel) is how the orchestrator and the operator follow a session without reading its terminal. For
Cursor it held tool calls and nothing said:

- **No text.** Cursor's hooks report tool calls but no reply (`afterAgentResponse` would duplicate
  rows and stays uninstalled), and `record_spoken_message` matched Claude Code's
  `UserPromptSubmit`/`Stop` by name in Rust. Local probe 2026-11-24 (session `01a20f1b`).
- **Cursor call ids embed a `\n`** (`call-…-composer_call_3AhseUhG\nfc_…_0`): a key of the activity
  log and of a delta lookup.
- **Lost activity on a Coder target in tmux** (session `01a20f05`). Three defects in the hook-log
  follower:
  1. For a tmux run on a target, `on_exit` fires when the **attached client** exits (a dropped
     `coder ssh`, a detach, a daemon restart), and its wrapper stopped the follower **and removed the
     hook log** while the agent kept running in tmux.
  2. A resume recreated the log with `write_file(b"")`, truncating whatever the follower had not yet
     read.
  3. `coder ssh` always runs its command in a pty (`onlcr`: `\n` → `\r\n`), and the follower counted
     **stream** bytes into its offset: one byte too many per line, so a reopened `tail -c +N` started
     past what was delivered — toward, or beyond, the log's end.

  Also: a resume's new follower replaced the registered one **without stopping it**, so two readers
  could deliver one log.

Cursor's own transcript is complete for text:
`~/.cursor/projects/<worktree slug>/agent-transcripts/<chat>/<chat>.jsonl`, lines like
`{"role":"user"|"assistant","message":{"content":[{"type":"text"|"tool_use",…}]}}` and
`{"type":"turn_ended",…}`. It carries **no tool results and no tool ids**.

## Change

### `coding_agent.activity` (spec YAML)

Where a session's activity comes from, per kind — `messages` and `tool_calls`, each
`hooks | transcript | both` (default `hooks`) — and, for a transcript, its path and how its lines map.
The mapping is **declared**, read by one generic mapper (`TranscriptSource::map_line`), reusing the
usage binding's `FieldPath` and `where` (`FieldEq`) predicates:

```yaml
activity:
  messages: transcript
  tool_calls: hooks
  hook_messages:                 # hook events that carry spoken text (Claude, Muse)
    - { event: UserPromptSubmit, field: prompt, role: user }
  transcript:
    path: "{agent_home}/.cursor/projects/{worktree_slug}/agent-transcripts/{conversation_id}/{conversation_id}.jsonl"
    lines:                       # first rule whose `where` holds maps the line
      - where: [{ field: role, equals: assistant }]
        parts: message.content   # each array element is matched against `emit`
        emit:
          - { kind: agent_message, where: [{ field: type, equals: text }], text: text }
          - { kind: tool_use, where: [{ field: type, equals: tool_use }], name: name, input: input }
      - where: [{ field: type, equals: turn_ended }]
        emit: [{ kind: turn_end }]
```

- `kind` ∈ `user_message | agent_message | tool_use | turn_end`; a message needs `text`, a tool use
  `name` (`input`, `id` optional). `unwrap_tag: X` keeps the trimmed inside of `<X>…</X>` (Cursor's
  `<timestamp>`/`<user_query>` wrapper). No regex (operator decision: not needed, and it would be a
  new dependency of the crate).
- Path variables: `agent_home`, `conversation_id`, `worktree`, `worktree_slug` (every run of
  non-alphanumerics made one `-`, trimmed — Cursor's project directory naming, checked against ~130
  local directories). A path naming another variable, a source naming an undeclared transcript, or an
  emit missing its value is refused at load.
- `cursor.yaml`: `messages: transcript`, `tool_calls: hooks`. `claude.yaml`/`muse.yaml`:
  `messages: hooks` with `hook_messages` for `UserPromptSubmit.prompt` / `Stop.last_assistant_message`
  — the Rust match on those names is gone. A spec with no `activity` section reads nothing said from
  hooks (an operator override of `claude.yaml` must carry the section to keep its messages).

### Following the transcript (`tddy-agent-launch`)

- **One follower** (`log_follower::FileFollower`) behind the hook log and the transcript, on a target
  through its remote shell and on this host through `LocalShell`.
  - The command turns a pty's output processing off (`stty -opost`), discards `tail`'s stderr (a
    transport merging it would put it on the stream), and uses `tail -F` for a file that may not exist
    yet (a transcript appears with the first prompt).
  - Its offset counts **delivered line bytes**, a `\r\n` on the stream counted as the file's one `\n`
    (every followed file is JSON lines): a torn line is re-read whole, and offsets stay exact behind a
    pty that kept `onlcr`.
  - `FollowOffset` is persisted under `<session_dir>/followers/` (`hooks.offset`,
    `transcript.offset`), so the follower a resume, a re-attach or a restarted daemon starts reads on
    from the last delivered byte — never from the start, never from the end.
  - Start, each break and each reopen are logged at INFO with the path and the offset.
- **Path placement.** Rendered from the session's recorded `usage-context.json` (conversation,
  worktree). On a target, `agent_home` is the target shell's `$HOME`, not the host home the usage
  context records; the worktree is the target's path.
- **Lifetime.** A target's transcript `tail` lives while the session's hook log exists there (a target
  session of an agent without hooks has no transcript followed — logged). A host session's lives
  while `<session_dir>/followers/transcript.alive` exists. Both followers are registered in
  `RemoteHookFollowers` (now per session: hook follower and transcript follower); a replaced follower
  is aborted.
- **tmux.** On a target in tmux, the client's exit keeps the followers (and the log) when
  `tmux has-session` says the agent's tmux session lives; otherwise, and for a non-tmux run, the exit
  stops them and removes the log. A host session in tmux registers no new exit hook (that would turn
  on `remain-on-exit`); its follower ends with the session's stop or delete.
- `create_remote_hook_log` keeps a log already there (resume). The hook offset is forgotten with the
  log at the session's end; the transcript offset is kept — Cursor's file outlives the session and a
  resume appends to the same chat.

### Recording (`tddy-session-activity`, `tddy-daemon-kernel`)

- `ActivityServiceImpl::record_transcript_line(TranscriptLine)` — reached through
  `HookReportDelivery::record_transcript_line` (the session host's delivery serves it; others refuse).
  Messages become `user_message_chunk` / `agent_message_chunk` frames in `acp-transcript.jsonl` and on
  the live hub, so `ReadSessionTranscript` and the Activity panel (which already renders both chunk
  kinds) show them.
- **Pairing** (`AgentActivityHub::tool_turns`): the transcript has no tool ids, so the Nth call of a
  tool one source reports in a turn pairs with the other's Nth. With `tool_calls: hooks` a paired
  `tool_use` records nothing (the hooks carry status and results); with `transcript|both` an unpaired
  one is a `completed` row (`transcript:<end offset>:<n>`, or its own id), and a hook row that pairs
  later **adopts that id** so the rows merge. Turns align on the hooks' `Done` status and the
  transcript's `turn_end`, so a follower that is behind does not pair across turns.
- **Order.** `ReadSessionTranscript` merges message frames and tool rows by timestamp, and a follower
  may read a line long after it was written. Rule: *what the transcript says next happened before
  every hook call it has not reached yet* — a message is stamped no later than the earliest such
  call's start, minus 1 ms. Live and behind followers both produce Cursor's order.
  **Accepted limit:** the replay places a tool call at its *completion*; text between two calls that
  ran **in parallel** is placed before the later call's start.
- **Dedupe** with `messages: both`: a per-session window of the last 64 (role, text) recorded drops
  the second sighting (an identical repeated prompt inside that window is dropped too).
- **Call ids:** `ReportAgentActivity` replaces every control or whitespace character of
  `tool_use_id` with `_`, on host and target alike.

### Not changed

- `activity_seq` stays the session room's poll tick, `NO_TICK` (0) in the durable log by design;
  followers page by `log_seq` (operator decision).
- The Activity panel needed no change; a component test pins it rendering transcript-sourced
  messages among tool calls.

## Follow-ups

- **Claude's intermediate text** (the text between tool calls reaches no Claude hook): an
  `activity.transcript` section in `claude.yaml` over `{agent_home}/.claude/projects/*/…` — the next
  PR, expected YAML-only bar Claude's string-content user lines.
- A host-mode **system** daemon reads the user's transcript as itself, as the usage reader does.
- A daemon restart does not re-attach target followers by itself; they resume (from the persisted
  offset) at the session's next resume or re-attach.

## Acceptance tests

| Test | Proves |
|---|---|
| `tddy-coding-agent-specs/tests/spec_activity_acceptance.rs` | YAML slot, real fixture mapped in order, slug, target-home path, load refusals |
| `tddy-session-activity/tests/cursor_transcript_activity.rs` | `ReadSessionTranscript` = user, agent, Read, Shell, Grep, agent for live and behind followers; prompt once; ids sanitised |
| `tddy-agent-launch/tests/transcript_follower_acceptance.rs` | target `$HOME` path, Coder pty CRLF + reopen at the file's byte, host follow before the file exists, tmux-style restart while writing: no loss, no duplicate |
| `tddy-agent-launch/tests/remote_hook_follower_acceptance.rs` | resume mid-line, persisted-offset resume by a new follower, pty CRLF, log kept on resume |
| `harness_launch` unit tests | tmux client exit keeps followers while tmux lives; stops them otherwise |
| `tddy-daemon-kernel` `tool_turns` unit tests | Nth-pairs-Nth, id adoption, turn alignment |
