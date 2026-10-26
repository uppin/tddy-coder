# 2026-10-25 — ReportSessionStatusRequest.message

**Type:** Feature

`#start-parity` 6/9 ([#755](https://github.com/uppin/tddy-coder/pull/755)). `activity.proto`: `ReportSessionStatusRequest.message = 5`, the hook
event's own message, sent with `WaitingForInput` and recorded as the session's pending question;
empty for every other status. Web bindings (`activity_pb.ts`) regenerated.
