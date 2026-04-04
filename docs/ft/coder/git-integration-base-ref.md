# Git integration base ref (worktrees)

**Product area:** Coder workflow — git worktrees after plan approval.

## Purpose

Workflow sessions that materialize a git worktree select a **remote-tracking integration base ref** (for example `origin/main` or `origin/master`). That ref is the fetch target and the start point for `git worktree add`.

## Behavior

### Default resolution (no project override)

When the workflow calls `setup_worktree_for_session` with only the repository root and session directory, **tddy-core** resolves the base ref after `git fetch origin`:

1. `origin/master` when that ref exists on the remote.
2. Otherwise `origin/main` when that ref exists.
3. Otherwise the symbolic ref `refs/remotes/origin/HEAD` when it points at a valid `origin/<branch>`.

### Project registry

Registered projects live in `~/.tddy/projects/projects.yaml`. Each row has optional **`main_branch_ref`**: a single remote-tracking ref of the form `origin/<branch-name>`.

- Rows **without** `main_branch_ref` use the documented default **`origin/master`** (same effective behavior as historical hardcoding).
- **`effective_integration_base_ref_for_project`** returns the stored ref or that default.
- **`add_project`** rejects invalid `main_branch_ref` values before any YAML write.

### Validation

Integration base strings must match `origin/<one segment>` with no shell metacharacters or `git` option injection patterns (`validate_integration_base_ref` in **tddy-core**).

## API and tooling surface

- **tddy-daemon** `ProjectData` includes optional `main_branch_ref` (serde).
- **CreateProject** paths that build `ProjectData` supply `main_branch_ref` when the connection API exposes it; otherwise the field is absent and the default applies at resolution time.

## Related

- [Project concept](../daemon/project-concept.md) — projects registry and session `project_id`.
- **tddy-core** `worktree` module — `fetch_integration_base`, `setup_worktree_for_session_with_integration_base`, `resolve_default_integration_base_ref`, `setup_worktree_for_session`.
- **tddy-daemon** `project_storage` — `effective_integration_base_ref_for_project`, `add_project`.

## Known follow-ups

End-to-end wiring from a session’s **`project_id`** to `setup_worktree_for_session_with_integration_base` with the resolved ref belongs in the daemon and workflow recipe hooks when those layers pass the effective ref into worktree setup.
