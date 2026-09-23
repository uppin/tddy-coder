# 2026-09-23 — Log lines with no explicit target moved to the carved crates' module paths

**Category:** Operational note — log filtering
**Source:** `#carve` 12/14 (`carve-core-facade`, PR #522), found at `/validate-changes`

The `log` macros take their target from `module_path!()` when no `target:` is given. #522 moved
every module out of `tddy-core`, so a line logged from, say, `presenter/workflow_runner.rs` carries
`tddy_presenter::presenter::workflow_runner` where it used to carry
`tddy_core::presenter::workflow_runner`. **A `RUST_LOG` or `log:` policy selecting `tddy_core` or
`tddy_core::<module>` no longer matches those lines.**

Explicit targets did **not** change. The 95 `target: "tddy_core::…"` sites in the moved code (all
of `session_action_pipeline` and `session_action_jobs`, most of the changeset's) keep their names,
which follows the precedent of `2026-09-10-log-targets-across-the-extracted-crates-still-name-tddy-daemon.md`.

Approximate production call sites with **no** explicit target, per crate: `tddy-presenter` 71,
`tddy-agent-backend` 57, `tddy-session-worktree` 34, `tddy-agent-skills` 31,
`tddy-workflow-engine` 19, `tddy-toolcall` 8, `tddy-changeset` 5. The count is a regex over
`trace|debug|info|warn|error!(` up to the first `#[cfg(test)]`.

No checked-in config or script selects on `tddy_core` (checked: `*.yaml`, `*.sh` and string
literals in `packages/*/src`). So nothing in the repo broke. The exposure is an operator's own
filter.

**What would close it.** Choose one policy for the whole `#carve`/`#unbundle` family: either pin
explicit legacy targets everywhere (keeps filters working, but the names describe history), or
rename every explicit target to its crate once, with a release note (the names describe structure,
and operators make one edit). Right now the carved crates mix both. Until that decision, announce
the implicit-target change with #522.
