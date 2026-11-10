# 2026-11-07 — The launch code `#agent-tools` left callerless is deleted

**Type:** Refactor

`#agent-tools` 24/28 ([#832](https://github.com/uppin/tddy-coder/pull/832)), branch
`feature/agent-tools/dead-launch-code`, based on `feature/agent-tools/bounded-waits`.

Packages: `tddy-agent-launch`, `tddy-sandbox-runner`, `tddy-daemon-sandbox`, `tddy-sandbox-app`,
`tddy-coder`; comment fixes in `tddy-acp-stub` and `tddy-integration-tests`; the QEMU guest init in
`tddy-sandbox-qemu`.

## What changed

- **`tddy-agent-launch`.** Deleted: `warm_up_jail_agents` and `jail_semantic_index_env`
  (`jail_launch_steps.rs`); the whole of `jail_worktree.rs` (`project_default_branch_ref`,
  `create_jail_project_worktree`, `link_jail_branch_to_stack_node`) with its `mod` line, its
  re-export and `JailBranch`; `create_routed_session_dir`; `chat::{cursor_notify_env,
  resumed_cursor_notify_env, cursor_hook_daemon_url}`; `relaunch_jail_steps`' `relaunch_jail_env`,
  `spawn_relaunched_runner` and `bridge_relaunched_jail` with `RelaunchJailEnv`,
  `RelaunchedRunnerSpawn` and `RelaunchedJailBridge`. `relaunch_managed_workflow`,
  `RelaunchManagedEnv` and `relaunch_jail_dirs` stay (live). `session_provider_route_tests` builds its
  claude-cli session directory with a local helper.
- **Runner.** `SandboxRunnerArgs` has no `--model`: the parser refuses it (`UnknownArgument`). The
  writers are gone from `sandbox_plan_builder` (`tddy-daemon-sandbox`) and `tddy-sandbox-app`'s
  `spawn.rs`, and from the QEMU guest's `S99tddy-sandbox`, which passed `--model none`. `params.model`
  in `spawn.rs` stays: the exec plan reads it.
- **`tddy-coder`.** `acp_from_spec` loses `claude_acp_backend_from_spec`, `codex_acp_backend_from_spec`,
  `opencode_acp_backend_from_spec` and `AcpLaunch.agent`; `acp_backends_from_spec_acceptance` builds
  its backends with `spec_backend_for`.
- **Tests.** `no_callerless_launch_helper_survives` (a source scan of `tddy-agent-launch/src`),
  `the_sandbox_plan_argv_carries_no_model_flag`, `the_runner_refuses_a_model_flag`. The macOS-only
  `SandboxHandle` import of `sandbox_stdio_seatbelt_acceptance.rs` is added; its two tests are `#[ignore]`d
  (see below).

## Resolved backlog

- `docs/dev/todo/2026-11-05-callerless-bespoke-launch-helpers.md`
- `docs/dev/todo/2026-11-03-runner-model-flag-is-vestigial.md`
- `docs/dev/todo/2026-11-05-cli-spec-features-leftovers.md` (stale; its ssh-pointer part closed in #829)
- `docs/dev/todo/2026-11-03-harness-clis-legacy-cli-harness-removal.md` and
  `docs/dev/todo/2026-11-05-model-listing-and-bespoke-acp-backends-remain.md` were already gone at the base.

## Decisions

- Deleting `jail_worktree.rs` and `AcpLaunch.agent` was approved by the developer (2026-11-07); so were
  the three `acp_from_spec` builders, the extra `--model` argv writers and the `SandboxHandle` import.
- The QEMU guest's `--model none` was found at wrap, not at planning: the compiler cannot see a shell
  script, and a runner without the flag would have refused to start in the guest.

## Left open

- `docs/dev/todo/2026-11-07-seatbelt-stdio-acceptance-fails-on-macos.md` — two `#[ignore]`d tests that fail
  on the parent commit too.
- `docs/dev/todo/2026-11-07-stale-claude-acp-backend-references.md` — docs that still name the deleted
  ACP backends; the sweep node's.

## Tests

`no_callerless_launch_helpers`, `tddy-sandbox-runner` lib, `tddy-daemon-sandbox` `sandbox_plan_builder`,
`tddy-coder` `acp_backends_from_spec_acceptance`; clippy `--all-targets -D warnings` clean on the seven
touched packages.
