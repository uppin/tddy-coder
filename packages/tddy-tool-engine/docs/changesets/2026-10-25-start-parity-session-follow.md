# 2026-10-25 — ReadSessionActivity and ReadSessionTranscript

**Type:** Feature

`#start-parity` 6/9 ([#755](https://github.com/uppin/tddy-coder/pull/755)). New `session_follow`:
`READ_SESSION_ACTIVITY`, `READ_SESSION_TRANSCRIPT`, `FOLLOW_PAGE_DEFAULT` (50), `FOLLOW_PAGE_MAX`
(200), `TRANSCRIPT_ITEM_MAX_BYTES` (4096), `FollowPage<T>`, `ActivityPage`, `TranscriptItem`,
`SessionActivity`, `ToolCallBodies`, `page_activity` (forward by `activity_seq`, `statuses`
filter), `page_transcript` (`newest_first` / `from_cursor`), `truncate_transcript_text`
(character boundary), `session_follow_tool_defs` and `run_session_follow_tool`.
`project_home_tools`: both are project-home target tools, with
`ProjectHomePort::{session_activity, session_transcript, session_tool_call}` and their dispatch.

Tests: `tests/session_follow_paging.rs`, `tests/project_home_target_host.rs`.
