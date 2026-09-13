# 2026-09-13 — `#unbundle` post-stack follow-ups (node 10)

**Stack:** `#unbundle` — node **10 of 10**, based on node 9
([#481](https://github.com/uppin/tddy-coder/pull/481) /
`feature/unbundle/daemon-becomes-wiring`).

Closes stack-documented defects that could not land on frozen predecessor nodes without
cascade-stale rewrites. Larger backlog items stay in `docs/dev/todo/` as later milestones.

## Responsibility

**Milestone 1 (this PR):**

| Todo | Fix |
|---|---|
| [Family T dropped from local socket](2026-09-10-family-t-was-dropped-from-the-local-socket-before-the-policy-existed.md) | Two-pass `livekit.proto` codegen in `tddy-service` (`tonic_livekit` + generated adapter); mount `LiveKitServiceServer` on the local Unix socket with a shared `Arc` from `runtime` (same pattern as RPC entry). |
| [Sandbox spawn argv grep stale path](2026-09-10-sandboxed-session-spawn-argv-greps-a-file-the-connection-service-split-emptied.md) | Point `sandbox_session_stdio_acceptance` at the three family-C spawn modules under `tddy-session-lifecycle`. |

**Later milestones (not in this PR):** stdio transport switch, desktop embed verification, node-1
changeset overclaim (#470), service.rs budget splits, in-jail suite wiring, auth/livekit file
budget, reflection/coder dispatch hygiene — see todos citing `#unbundle` under `docs/dev/todo/`.

## Boundaries

- No edits to merged predecessor branches (nodes 1–9).
- No behaviour change beyond restoring LiveKit on the local socket and fixing the compile-time grep.
- Does not relocate `sandbox_session_stdio_acceptance.rs` into `tddy-daemon-sandbox` (follow-up).

## Dependencies

- Node 9 landed wiring: multi-service local socket, node 6 generator, `tddy-daemon-livekit`
  `LiveKitServiceImpl`.

## Draft PR contract

- Base: `feature/unbundle/daemon-becomes-wiring`.
- Title: `fix(daemon,service,livekit): restore LiveKit on local socket and fix sandbox argv test (#unbundle 10/10)`.
- Scoped verification: `./test -p tddy-service -p tddy-daemon-livekit -p tddy-daemon` plus
  `--test local_socket_reachability_acceptance`, `--test local_token_uds`, and
  `sandboxed_session_spawn_argv_carries_stdio_and_no_grpc_flags` in
  `sandbox_session_stdio_acceptance` (real-jail test remains environment-sensitive on macOS).
- After merge: remove or archive the two M1 todo files; re-register stack as 10 nodes via
  `gh stack link --base master`.
