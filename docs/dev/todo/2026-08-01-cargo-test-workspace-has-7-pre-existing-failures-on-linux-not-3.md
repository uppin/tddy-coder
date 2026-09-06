# 2026-08-01 — `cargo test --workspace` has 7 pre-existing failures on Linux, not 3

**Category:** Known failing test
**Source:** session-attach-ui wrap, 2026-08-01

A full `cargo test --workspace --no-fail-fast` run on 2026-08-01 was **513 suites pass, 7 fail**, and
every failure predates that branch — established by code location, not assumption. Worth recording
because the workspace suite is therefore not a clean signal for anyone, and a real regression would be
easy to lose among these:

- **The sandbox set** (5) — `sandboxed_bash_action_writes_to_output_dir`,
  `sandboxed_claude_cli_starts_on_linux_with_the_cgroups_backend`, the three
  `sandboxed_cursor_cli_*`, plus `cursor_cli_sandbox_start_succeeds_when_sandbox_backend_available`:
  the test scripts do not build `tddy-sandbox-runner`. `start_session_sandbox_unsupported_on_non_darwin`
  is macOS-only.
  **Refinement (2026-08-03):** building `tddy-sandbox-runner` first is not sufficient. With the runner
  built they still fail, with `spawn sandbox runner in cgroups jail failed: Operation not permitted
  (os error 1)`, and the self-skip that should cover it does not fire.
  **The error's parenthetical ("the host may forbid unprivileged user namespaces") is a red herring,
  and it misled an earlier diagnosis here.** This host has
  `kernel.apparmor_restrict_unprivileged_userns=1` *and*
  `apparmor_restrict_unprivileged_unconfined=0` — the second exempts unconfined processes, and a
  `cargo test` binary is unconfined, so `unshare(CLONE_NEWUSER)` is in fact **permitted**. Verified
  directly: the supervisor's own jail (the same `unshare` + uid/gid-map sequence) runs to completion
  on this host, producing `uid=0(root)` inside the namespace.
  So the `EPERM` comes from a **later** step, most plausibly the cgroup write — cgroup v2 delegation
  containment, which `packages/tddy-sandbox/docs/architecture.md` already documents as the reason an
  unprivileged process cannot place its own child in a limited scope. Whoever picks this up should
  make the error name the syscall that actually failed before theorising further; see
  "`unprivileged_userns_available()` under-approximates what the jail needs" below.
- `session_token::tests::verify_rejects_a_token_with_a_tampered_signature` — `packages/tddy-github`.
  Root cause found 2026-08-02: a ~1-in-64 base64 canonicalization flake, not a signature bug. See
  the dedicated entry under Future Enhancements for the exact mechanism and the fix.
- `cursor_cli::tests::cursor_agent_prerequisite_reads_include_install_dir_and_share_root` —
  `packages/tddy-sandbox-recipes`.
- `cursor_cli_peer_spawn_records_the_orchestrator_link_even_without_repo_path` — already tracked above.
- `cancel_task_cancels_a_bash_pty_task` (`task_service_acceptance.rs`) — PTY timing.

**Re-measured 2026-08-13 (pr-stack-base-session wrap): 11 suites / 24 tests**, every one attributed by
its own failure message, none from that branch. New entries beyond the list above:

- **The ACP-stub set** (8) — the six `acp_*` in `tddy-integration-tests`
  (`acp_backend_acceptance`, `acp_host_bridge_acceptance`) and the two `codex_acp_backend_*`: they abort
  with `tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub`. This is the **same class of gap as
  `tddy-sandbox-runner`** — a fixture binary no test script builds — and it is the larger half of the
  workspace's noise. Both belong in whatever `./test` does before it runs.
- `factory_is_shared_per_room_so_two_clients_to_one_peer_never_collide` (`tddy-livekit`) — testcontainers
  loses a UDP port race: `failed to bind host port 0.0.0.0:<port>/udp: address already in use`. Use
  `./run-livekit-testkit-server` and `LIVEKIT_TESTKIT_WS_URL` to avoid it.
- `sandbox_runner_streams_demo_tui_dimensions_on_session_channel` (`tddy-sandbox-darwin`) — macOS-only.
- The sandbox set is **four** `sandboxed_cursor_cli_*`, not three (`..._connect_session_returns_empty_livekit`,
  `..._start_persists_metadata_and_empty_livekit`, `..._start_wires_specialized_agents_env_and_metadata`,
  `..._terminal_io_round_trips`), and on an unprivileged user the cgroups ones fail with
  `Operation not permitted … the host may forbid unprivileged user namespaces` rather than a missing binary.
- `verify_rejects_a_token_with_a_tampered_signature` is **genuinely flaky**, not consistently failing:
  it failed once in five consecutive runs. It expects `InvalidSignature` and intermittently gets
  `Malformed`, so the tampering helper sometimes produces a string that fails base64 decoding before the
  signature is ever checked. Fix the helper to mutate within the alphabet.
