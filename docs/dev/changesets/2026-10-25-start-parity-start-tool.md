# 2026-10-25 — StartSession takes every New-session option, on any host

**Type:** Feature

`#start-parity` 2/9 ([#751](https://github.com/uppin/tddy-coder/pull/751)). The project-home
`StartProjectSession` exec tool (five arguments, own host only) is replaced by **`StartSession`**,
whose arguments are `StartSessionRequest`'s own fields — every option the New-session form can send.
Its schema is generated, and two parity tests keep it in step with the proto descriptor and with the
form's request builder. The orchestrator link moves into the request (`orchestrator_session_id = 45`,
`orchestrator_daemon_instance_id = 46`), so the daemon that creates the session records it, on any
host. Across:

- `tddy-service` — the two `StartSessionRequest` fields; regenerated `session_pb.ts`.
- `tddy-tool-engine` — `start_session_args` (args model, context-filled and withheld field tables,
  `into_request`) and `start_session_args/schema.rs` (the generated schema); `project_home_tools`
  offers `StartSession` with the daemon's choices.
- `tddy-agent-launch` — the start core records the orchestrator (`orchestrator_record.rs`) and refuses
  an orchestrated `tool` start; `classify_start_placement` / `managed_recipe_for` moved to
  `placement.rs`.
- `tddy-changeset` — `Changeset.orchestrator_daemon_instance_id`.
- `tddy-session-lifecycle` — `start_choices`, the eligible hosts and recipes a `ListExecTools` answer
  offers.
- `tddy-daemon-rpc` — `DaemonProjectHome::start_session` maps the arguments onto the request; the
  shared two-host test world (`tests/support/mod.rs`).
- `tddy-web` — the parity spec `createSessionRequestParity.cy.ts` and its Rust-written fixture.

Restructuring (engine moves, `/pr-wrap` file-length gate): `start_session_args.rs` 540 → 257
production lines (schema to `start_session_args/schema.rs`, 292); `svc_start_session_core.rs`
498 → 563 → 486.

Code issues: added `complexity-start-session-args-schema-schema-offering` (178 lines); re-measured
`handle_rpc` 188 → 192, tool-engine `lib.rs` 877 → 878 (stack-shared, not split),
`start_session_on_request` 391 → 79 + `start_here` 334; **closed**
`oversized-file-project-home-tools` — `project_home_tools.rs` 500 → 435 production lines.

Deferred (backlog entries added): an orchestrated `tool` start is refused
(`2026-10-25-orchestrated-tool-session-start-refused`); the orchestrator is written after the start
returns (`2026-10-25-start-session-orchestrator-recorded-after-start`); the agent's MCP schema has no
host or recipe enums (`2026-10-25-start-session-mcp-schema-has-no-host-or-recipe-choices`); the form
offers 8 of 10 recipes (`2026-10-25-new-session-form-offers-8-of-10-recipes`).
