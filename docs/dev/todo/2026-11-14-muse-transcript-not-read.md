# 2026-11-14 — `ReadSessionTranscript` is empty for a Muse Code session

**Category:** Known defect
**Source:** project-home orchestrator driving `muse-cli` session `01a1d89b-0e66-7880-bed2-a6bec7ba7238`
on 2026-11-14

`ReadSessionTranscript` returned no items, even after Muse finished its turn.

## Root cause

The transcript is built from tddy's own stores, never from the agent's.
`read_session_transcript` (`packages/tddy-service/src/acp_replay.rs:88-112`) merges
`acp-transcript.jsonl` (written by the tddy-coder presenter only, `:90-92`) with
`agent-activity.jsonl` (hooks). A Muse PTY session writes neither. Even with hooks wired
([2026-11-14-muse-hooks-not-wired.md](2026-11-14-muse-hooks-not-wired.md)) it would hold tool calls
only, no messages or reasoning, which is the limitation a `claude-cli` session has today.

`muse.yaml`'s `reads: []` is not a transcript source: `reads` is extra host paths a sandboxed tool
needs readable (`packages/tddy-actions/src/tool/def.rs:31-33`). The only agent-file readers in specs
are `usage:` bindings (`tddy-usage`), and they read token counts only.

## Where Muse keeps the conversation

- `~/.local/share/muse/sessions/YYYY/MM/DD/<muse-session-uuid>/session.jsonl`. For the session above:
  `~/.local/share/muse/sessions/2026/11/14/01a1d89b-27ee-7fd0-9a7e-c1723ee1a1b5/session.jsonl`.
  Subagents under `…/subagent/<uuid>/session.jsonl`, an index in
  `~/.local/share/muse/session-index.db`.
- The Muse session id is not the tddy one. Match the `runtime.session.metadata` record's
  `payload.record.workspace_root` to the session's worktree, as `codex.yaml`'s usage binding matches
  `payload.cwd`. A sandboxed run keeps it under the jail's `$HOME` instead (`home: { kind: session }`).
- JSONL envelopes `{schema_version, id, stream, sequence, recorded_at (µs), record_type, payload_type,
  payload_schema_version, payload}`. The records a transcript needs (`payload.event.kind`):

| Record | Transcript item |
|---|---|
| `runtime.user_intent.accepted` (`refill_blocks[].text`) | user message |
| `assistant_message_committed` (`text`) | agent message |
| `reasoning_summary_committed` | agent thought |
| `assistant_tool_calls_committed` (`tool_calls[].{call_id,name,args}`) | tool call |
| `tool_result_batch_committed` (`results[].{tool_call_id,text}`) | tool call update |
| `user_input_prompt_requested` (`prompt_id`, `tool_call_id`, `questions[]`) | question asked |
| `user_input_prompt_settled` (`outcome`, `answers[].{id,selected_label}`) | question answered (here: Green, Banana) |
| `terminal` (`terminal: completed`) | turn end |

- `muse export --session <id|path>` writes a documented, versioned JSON document
  (`export_schema_version 1`: messages, tool calls/results, approvals, question outcomes).

## What remains

Recommended: a declarative `transcript:` binding in the spec, beside `usage:`, reusing `tddy-usage`'s
`jsonl` source / `match` / `where` machinery, with Muse's `session.jsonl` as its first binding and
Claude / Codex / OpenCode after. Agent facts stay in YAML and the engine knows formats.

Alternatives: a Muse-specific reader feeding `read_session_transcript` a third source (smaller, but
couples Rust to Muse's internal schema; leave a TODO to lift it into the binding), or the same mapping
over `muse export` (stabler schema, but a subprocess and a whole-document read per call).

The same reader could also supply the pending question when hooks cannot (an unsettled
`user_input_prompt_requested`), at the cost of a second publisher beside the hook path.

**Why deferred:** found by investigation only; the binding needs design.
