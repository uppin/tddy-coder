# 2026-09-09 — Two protos appear, seventeen rpcs leave connection.proto, and a constant moves for a binary's sake

`session_agents.proto` (9 rpcs) and `activity.proto` (8) are added, both importing `types.proto`;
`connection.proto` loses 17 rpcs and the messages that went with them, ending at 33.

**No `reserved` field numbers were needed**, and that is worth stating because the plan expected
otherwise. Protobuf has no `reserved` for service *methods*, and no field number on a surviving
message was vacated. `connection.proto`'s header note is the standing record that these coordinates
once answered there and must not be re-declared.

`types.proto` gains `SessionAgentStatus` and `SessionAgentActivity` — the only genuinely shared
types this node found, established by walking field types exactly as node 6 established
`HostDocumentScope`. `ListSessions` stays on `connection` and reaches both through `SessionEntry`,
which keeps field numbers 31 and 32 and now points at the shared declarations.

Two constant modules are the interesting part:

- **`session_agents::IN_JAIL_RELAYABLE`** — the five `(service, method)` pairs an in-jail agent may
  relay to its host, plus `SESSION_AGENT_SERVICE`. Declared here rather than in `tddy-session-agents`
  because the second reader is `tddy-sandbox-runner`, which runs inside every jail.
- **`session_activity::{NO_TICK, FIRST_TICK, next_tick, ACTIVITY_SERVICE}`** — the delta tick rule,
  with `const _: () = assert!(FIRST_TICK != NO_TICK);` enforcing the invariant at compile time. They
  started in `tddy-session-activity`; putting them there made `tddy-session-sync`, a standalone
  installed binary, pull in `teloxide`, `teloxide-core`, `teloxide-macros`, `tddy-telegram` and
  `tddy-github` to read a `const u64 = 0`. Moving them here takes
  `cargo tree -p tddy-session-sync | grep -ci teloxide` from 3 to 0.

One principle behind both: **a constant read by a process that must stay small belongs beside the
proto, not beside the server.**

`extern crate self as tddy_service;` was added to `lib.rs`. Node 6's `generate_tonic_adapter` emits
`tddy_service::to_tonic_status` and the path is deliberately not configurable, so the first adapters
generated into this crate's own `OUT_DIR` do not compile as emitted. Reported upward rather than
fixed here — see [`docs/dev/todo/2026-09-12-generate-tonic-adapter-hardcodes-its-status-conversion-path.md`](../../../../docs/dev/todo/2026-09-12-generate-tonic-adapter-hardcodes-its-status-conversion-path.md).

Full record: [../../../../docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md](../../../../docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md).
