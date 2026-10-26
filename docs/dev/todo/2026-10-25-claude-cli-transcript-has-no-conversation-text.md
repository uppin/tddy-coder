# 2026-10-25 — A claude-cli session's transcript holds tool calls only

**Category:** Gap (medium)
**Source:** `#start-parity` 6/9 (`feature/start-parity/session-follow`, PR #755)

- `ReadSessionTranscript` (and the web's Activities view, through `GetAcpReplayPage`) reads
  `tddy_service::acp_replay::read_session_transcript`, which merges `acp-transcript.jsonl` with the
  session's `agent-activity.jsonl`. Only a `tool` session writes `acp-transcript.jsonl`; a
  claude-cli session reports tool calls through its hooks and nothing else, so its transcript has
  no user prompts, assistant messages or thoughts.
- An orchestrator following a claude-cli child therefore sees what it ran, and the question it
  waits on (`ReadSessionActivity`'s `pending_question`), but not what it said.
- **Why kept:** the node's `## Boundaries` left it out — the history does not exist server-side,
  and producing it is new capture, not a read.
- **What would close it:** capture the conversation text for claude-cli sessions on the server —
  e.g. a `UserPromptSubmit` / `Stop` hook reporting the prompt and the last assistant message, or
  tailing the harness's own transcript file named in the hook payload (`transcript_path`) — into
  `acp-transcript.jsonl` as the ACP `user_message_chunk` / `agent_message_chunk` frames a tool
  session writes, so both readers pick it up unchanged. Acceptance beside
  `packages/tddy-daemon-rpc/tests/session_follow_acceptance.rs`.
