# 2026-09-13 — `#unbundle` post-stack follow-ups (node 10)

**Type:** Hygiene / defect closure

Node **10 of 10** on the `#unbundle` stack ([#482](https://github.com/uppin/tddy-coder/pull/482), based on
[#481](https://github.com/uppin/tddy-coder/pull/481)). Closes actionable post-stack todos without
reopening frozen nodes 1–9. Large follow-ups (stdio transport switch, versioned protos, file-budget
splits, desktop, …) stay in `docs/dev/todo/`.

## What landed

| Milestone | Change |
|-----------|--------|
| M1 | LiveKit two-pass codegen + local UDS mount; sandbox spawn argv grep → `tddy-session-lifecycle` modules |
| M2 | PTY relay stdin pump / teardown; `#[serial(livekit_docker)]`; stdio `signal_start_ready` before first frame |
| M3 | Remove `StubEligibleDaemonSource`; generated host/worktree tonic adapters in `tddy-session-lifecycle` |
| M5 | Reflection merges supplemental descriptor sets; `terminal_session` bytes from `tddy-terminal-rpc`; daemon registers supplements |
| M6 | `MAX_MANIFEST_BYTES` / `env_non_empty` from `tddy-core`; retire `tddy-tools::relay`; workflow-recipes log targets |
| M7 | LiveKit `create_room` 30s timeout; screen-sharing vault `write_atomic_with_mode(0o600)` |

Eleven resolved todo entries under `docs/dev/todo/` were removed with this PR. The vault truncate todo
was narrowed to note VNC removal and screen-sharing conversion.

## Post-unbundle backlog (unchanged scope)

Still tracked in `docs/dev/todo/` — not part of node 10:

- Transport / sandbox: full stdio switch, stdio acceptance relocation, in-jail Linux story
- Architecture: restructure defects, versioned protos, proto crate / TUI dep, dependency harness, module splits
- Product: Tauri desktop, roster/ACP/web gaps, VNC decision, broader LiveKit deadlines
- Hygiene: exec-tool in-process fixture, ACP dedup, coder dispatch, permission NDJSON, `buildId.ts`

## Verification

Scoped local: `./test -p tddy-service` (reflection acceptance), `tddy-stdio`, `tddy-screen-sharing`,
`tddy-terminal-rpc`, `tddy-daemon-sandbox` lib tests; full stack gate on CI for #482.
