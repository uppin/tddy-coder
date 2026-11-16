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
