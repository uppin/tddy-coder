# 2026-11-16 — The stand-in target, stub agents and daemon helpers are repeated across the remote-shell test files

**Category:** Test debt (left by the `#remote-shell` stack, #915–#921)
**Source:** `packages/tddy-agent-launch/tests/{remote_agent_launch,remote_hooks,coder_agent_launch}_acceptance.rs`, `packages/tddy-session-lifecycle/tests/{remote_target_session,derived_host_start,remote_hooks_session,coder_target_session}_acceptance.rs`

Each PR of the stack wrote its acceptance tests while the PR below it was frozen or being
implemented, and a test file's fixture lived inside the file. So the stand-in `ssh` that runs the
remote command locally (`ATarget`), the stub agents, the `acme` tool spec, the daemon helpers and
the guarded fixture warm-up (`TDDY_FIXTURE_WARMUP`) exist in up to seven copies. The stand-in
`coder` and the Coder vault stand-ins are shared already (`tddy_remote_shell` test support,
`tddy_host_service::test_util`).

A change to how a target session starts now has to be made in every copy, and #920's warm-up fix
shows the cost: it touched five files to change one fixture.

**Why deferred:** the files belong to four PRs that were finished one after another; moving a
fixture out of a lower PR's file from a higher one would have rewritten a frozen PR's tests. The
stack's sweep (#remote-shell 8/8) is a production restructure and leaves test files alone.

**What would close it:** one support module per crate (`tests/support/remote_target.rs` in
`tddy-agent-launch` and in `tddy-session-lifecycle`, or a shared one in `tddy-testing-commons`)
holding the stand-in target, the stub agents and the warm-up, with the seven files importing it and
their assertions unchanged.

**Grown 2026-11-18 (`#coder-start` 3/3, #942):** `packages/tddy-session-driver/tests/session_driver_acceptance.rs`
holds an eighth copy of the `acme` tool file, the stub agents and the guarded warm-up, plus a
stand-in `coder` **script**. The shared stand-in could not be used: the suite drives the compiled
binary as a subprocess, so it needs a `coder` executable on disk, and `tddy_remote_shell`'s test
support offers only the in-process `InMemoryShell`. A shared support module should therefore also
offer the stand-in `coder` as a script writer.

**Narrowed 2026-11-18 (`#coder-start` 9/10, #949):** `tddy_testing_commons::remote_target` now holds
`write_executable` with the warm-up (all 11 copies replaced) and `StandInSshTarget` (the identical
`AReachableMachine`/`ATarget` in the four lifecycle files). What remains: the stand-in coder
`AWorkspace` (lifecycle `coder_target_session`, agent-launch `coder_agent_launch` — bodies diverged),
agent-launch's `ATarget` with its call log, the stub agent families, the `acme` spec, the daemon
helpers, and the copies in `tddy-remote-shell/tests/support` and `tddy-agent-launch/src/remote_agent_launch.rs`
(a `tddy-testing-commons` dev-dependency there risks a cycle).

**Narrowed 2026-11-19 (`#coder-start` 10/10, the sweep):** the copies that were still identical
line for line are consolidated. `tddy-session-lifecycle/tests/derived_host_support` holds the
derived-host daemon (`AnOperatorHome`, the stub agent, the host service, `ADaemonReachingAnSshHost`
with four of its methods) for `derived_host_start` and `orchestrator_target_start`, and
`tddy-agent-launch/tests/acme_target_support` holds the `acme` spec and host agent, the hook sink,
`InstalledHooks` and `ADaemon` for `coder_agent_launch` and `remote_hooks` (640 lines removed, 37
added in the suites). What remains has **drifted**, so merging it means choosing one behaviour for
every caller: the stand-in coder `AWorkspace` (two bodies), agent-launch's `ATarget`s (different
call logs), the stub-agent families (their run lines, trap and banner differ), `AnAgentRun`
(identical in `coder_target_session` and `remote_target_session`, 15 lines, but the two files share
no support module), and the copies in `tddy-remote-shell/tests/support` and
`tddy-agent-launch/src/remote_agent_launch.rs` (a `tddy-testing-commons` dev-dependency there risks a cycle).

**Narrowed 2026-11-21 (`#session-liveness` 6/8, #965):** the stand-in coder `AWorkspace` of
`tddy-agent-launch` moved out of `coder_agent_launch_acceptance.rs` into `tests/coder_workspace_support`,
shared by that suite and `target_shell_tab_acceptance` (one body, not two; the target tab's `-l` login shell
is a branch of it). The lifecycle `coder_target_session` copy is still separate and still differs.
