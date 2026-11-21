# 2026-11-21 — A claude-cli session's prompt and reply reach a live transcript tail

**Type:** Feature · **Complexity:** low (3 crates, one new channel, no wire change) · **Packages:** `tddy-daemon-kernel`, `tddy-session-activity`, `tddy-service`
Feature: [claude-cli-session.md § The transcript holds what was said](../../ft/daemon/claude-cli-session.md)

## State A

`record_spoken_message` appends the prompt and the reply to `acp-transcript.jsonl`, but a live
`StreamAcpReplay` follows `AgentActivityHub`, which broadcasts `AgentActivityRecord` only. A client
following live sees the text on its next re-read, not as it happens.
(todo `2026-11-21-transcript-text-has-no-live-tail.md`, resolved here and deleted)

## State B

- `AgentActivityHub` gains a second per-session broadcast of `AcpAgentMessage` (`subscribe_frames`,
  `publish_frame`).
- `record_spoken_message` publishes the frame it appends.
- `relay_acp_replay` merges both receivers; a message frame takes the next replay position.

## Decisions & Trade-offs

- **Second channel, not a widened item.** `subscribe` is used by the sandbox, the session-agents
  recorder and three relays; changing its item type touches all of them for one consumer.
- **Ordering across two channels** is restored in the relay: after a wake it drains both receivers
  and emits by recorded timestamp, a message winning a tie — the rule `read_session_transcript`
  merges by — so a live position equals the re-read position.
- `relay_acp_replay_count` and `relay_agent_activity` are unchanged: messages are not activity entries.
- `tddy_service::acp_replay::frame_timestamp` became `pub` for the relay.
- No proto change: the frame goes out as the existing `AcpReplayFrame`.

## Tests

`tddy-session-activity` `claude_cli_transcript_messages`:
`a_live_tail_receives_the_prompt_and_the_reply_among_the_tool_calls`. Kernel unit:
`delivers_a_published_frame_only_to_a_frame_subscriber_of_the_same_session`.
