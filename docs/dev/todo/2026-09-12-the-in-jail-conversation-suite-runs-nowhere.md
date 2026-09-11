# 2026-09-12 — `in_jail_conversation_acceptance.rs` runs on no machine anyone has

**Category:** Known failing test
**Source:** `#unbundle` node 7, [#476](https://github.com/uppin/tddy-coder/pull/476), changeset
[`2026-09-09-unbundle-session-agent-services`](../changesets/2026-09-09-unbundle-session-agent-services.md)

`packages/tddy-daemon/tests/in_jail_conversation_acceptance.rs` is the suite that was supposed to be
the proof for node 7's security-relevant edit: an agent **inside a real jail** opening, prompting and
cancelling a subagent conversation through the relay allowlist, at the coordinate family B moved to.
It is written, it is thorough, and it currently proves nothing.

**On Linux it does not run.** The file carries `#![cfg(target_os = "macos")]` as an inner attribute,
so on Ubuntu CI the whole test binary compiles to an empty one. `cargo test` reports it as a binary
with 0 tests — not as skipped, not as ignored. Nothing in a CI summary distinguishes that from a
suite that has no cases yet.

**On macOS it fails in setup.** Its own `FIXME(sandbox-stdio-attach)` records the measurement: the
jail's end of the `SessionChannel` does not reliably attach, and when it does not, nothing relayed
out of the jail can be answered — the runner refuses with *"the sandbox session channel is not
connected to the host daemon yet"* while its tool-IPC socket answers normally and its own log shows
`SandboxService serving over stdio`. The first run in a fresh process attaches in ~50 ms and the
conversation completes in ~0.5 s; back-to-back repeats then fail about three runs in four.

## It is not family B's defect

The untouched `sandbox_session_stdio_acceptance::real_daemon_session_drives_a_seatbelt_jailed_sandbox_runner_entirely_over_stdio`
fails the same way, 1 run in 3. And with `NullRpcHandler` in place of the daemon's handler, node 7's
probe comes back with that handler's own *"this host does not serve
session_agents.SessionAgentService/StreamSessionAgents"* — which is the relay carrying a family-B call
to the host and the refusal back, at the new coordinate. The defect is in the stdio bridge
(`tddy-daemon-sandbox::bridge_sandbox_stdio` and the runner's `StdioEndpoint::from_process_stdio`).

Node 7 reported it rather than working around it: a setup retry would hide a real defect behind a
suite that looks green, and the retry experiment showed the conversation timing out even after a
later attempt attached.

## What stands in for it meanwhile

- `tddy-session-agents`' `src/lib.rs` unit tests pin the permitted operation set and that every tuple
  names the new service. They cannot catch a mismatch between the allowlist and what is *served*.
- `packages/tddy-sandbox-runner/src/runner.rs`'s own tests read the same constant, so they agree with
  it by construction.

That leaves the one assertion that distinguishes "the allowlist was updated" from "the allowlist was
updated correctly" un-run on every machine in the pipeline.

## What closing it would take

Fix the stdio-bridge attach race first — it gates this suite and `sandbox_session_stdio_acceptance`
equally. Then decide what the Linux story is: either a Linux sandbox backend the suite can drive, or
an explicit `#[ignore]` with a name that says *why*, so a zero-test binary stops reading as a passing
one.
