# 2026-07-01 — **`sandbox.proto`: `SessionEnded` frame

**Type:** Fix

pty exit signal on `SessionChannel`** — new `SessionEnded { exit_code }` message + oneof field 13 on `SessionFrame` (sandbox → host): the pty command has exited, so the host stops polling and lets its stream ends drop for graceful shutdown. Delivery is always deferred to the next `HostPoll` reply (see `tddy-sandbox-runner`), never pushed immediately, to avoid racing ahead of queued terminal output. (tddy-service, tddy-daemon, tddy-sandbox-runner)
