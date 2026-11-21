# 2026-11-21 — A live `StreamAcpReplay` tail does not carry a claude-cli session's messages

**Category:** Gap (low)
**Source:** `#target-parity` 3/7 (`feature/target-parity/transcript-text`, PR #971)

- A claude-cli session's prompt and reply are appended to `acp-transcript.jsonl` by
  `ActivityServiceImpl::record_spoken_message` (`packages/tddy-session-activity/src/service/transcript_messages.rs`),
  so `ReadSessionTranscript` and `GetAcpReplayPage` return them.
- `StreamAcpReplay`'s live mode (`relay_acp_replay`, `packages/tddy-session-activity/src/streams.rs`)
  subscribes to `tddy_daemon_kernel::AgentActivityHub`, whose broadcast carries `AgentActivityRecord`
  only. A message frame is not a record, so a client following live sees the new text on its next
  re-read, not as it happens.
- **Why kept:** closing it changes the hub's channel type (or adds a second channel) in
  `tddy-daemon-kernel`, whose publishers (sandbox, hook reports) and every relay signature
  (`relay_acp_replay`, `relay_acp_replay_count`, `relay_agent_activity`) follow — a cross-crate
  change, outside this node's one crate and its `## Boundaries` ("does not change the transcript
  readers").
- **What would close it:** a second per-session broadcast on the hub carrying ACP frames (or
  widening the hub's item to an enum of record | frame); `record_spoken_message` publishes the frame
  it appends; `relay_acp_replay` merges both receivers, numbering the message frame as the next
  position. Acceptance: a `StreamAcpReplay` live subscriber receives the user-message frame after a
  `UserPromptSubmit` report.
