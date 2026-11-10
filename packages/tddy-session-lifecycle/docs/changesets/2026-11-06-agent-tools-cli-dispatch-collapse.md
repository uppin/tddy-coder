# 2026-11-06 — The claude and cursor suites run on the one CLI launch

**Type:** Refactor (tests)

`#agent-tools` 17/19 ([#823](https://github.com/uppin/tddy-coder/pull/823)).

- The claude-cli and cursor-cli suites are re-pointed at the generic launch, and `cursor_cli_session_acceptance` starts through `StartSession`. `test_util::builtin_coding_agents_for(config)` resolves `tddy-tools` the way a daemon does.
- New: `no_agent_name_gates::no_agent_id_literal_is_compared_or_dispatched_on_in_agent_launch`, `connection_service/stack_child_spawn_agent_tests.rs` (a child runs its parent's agent; no agents → `FAILED_PRECONDITION`), re-attach from the installed agents, and `cli_spec_features_acceptance::a_managed_claude_copys_shell_run_tddy_tools_submit_reaches_its_session_with_no_tddy_variable_in_the_agent`. `agent_registry_support`'s stub can run `tddy-tools submit` from its own shell.
- The operator-notify exemption is gone from every "the agent sees no `TDDY_*`" assertion.
- `stack_child_spawn_tests`' stub records its command line by rename (a torn read flaked one test per run).
- `DaemonSessionHost::coding_agents`: `None` refuses every CLI agent's start.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-06-agent-tools-cli-dispatch-collapse.md).
