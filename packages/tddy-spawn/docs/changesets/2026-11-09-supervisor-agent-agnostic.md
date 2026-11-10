# 2026-11-09 — A supervised clone is one SpawnStreamed of tddy-git-clone

**Type:** Architecture

`#agent-tools` 32/32 ([#865](https://github.com/uppin/tddy-coder/pull/865)).

- `supervisor_spawn::plan_supervised_clone(os_user, clone_helper, git, git_url, destination, env)`
  builds the one `SpawnStreamed` request of a clone: `tddy-git-clone --git <git> -- <url>
  <destination>` as the session user, with `env` whole and no scope. It refuses a non-absolute
  helper, git or destination.
- `clone_repo_via_supervisor_with_progress` sends that request. The helper is found through
  `TddyToolchain`, beside the daemon. `git_for_supervised_clone` takes git from the spawn-child
  `PATH`. A missing helper or git is a "failed precondition" refusal, with no bare `git` fallback.
  `Line` events are parsed by `git_progress` and throttled as for a local clone.
- `CloneSshEnv::ALLOWED_ENV_KEYS` is deleted. A clone's SSH env passes whole under the supervisor's
  env rule.
- Tests: `tests/supervised_clone_plan.rs`. The helper's own tests are
  `packages/tddy-git-clone/tests/clone_helper.rs`.
- Docs: [clone-run.md](../clone-run.md), [clone-ssh-environment.md](../clone-ssh-environment.md) and
  [commit-identity.md](../commit-identity.md).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-09-supervisor-agent-agnostic.md).
