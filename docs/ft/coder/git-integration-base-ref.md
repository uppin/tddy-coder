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

## Chain PR optional base (multi-segment `origin/...`)

Follow-up work can branch from an open remote branch (a **chain PR**) instead of the default integration base. The workflow remains opt-in at the API layer: callers pass an optional remote-tracking ref into **`setup_worktree_for_session_with_optional_chain_base`**.

### Ref shape and validation

- **`validate_integration_base_ref`** continues to govern single-segment project defaults: **`origin/<one-segment>`** only.
- **`validate_chain_pr_integration_base_ref`** accepts **`origin/<path>`** where **`path`** contains one or more non-empty segments separated by **`/`** (for example **`origin/feature/foo`**). The same class of unsafe characters as the single-segment validator is rejected, along with **`..`**, **`--`**, and whitespace in the path.

### Fetch and worktree creation

- **`fetch_chain_pr_integration_base`** validates, then runs **`git fetch origin <path>`** with the path after **`origin/`** (no shell).
- **`setup_worktree_for_session_with_optional_chain_base(repo_root, session_dir, optional_chain_base_ref)`**:
  - With **`None`**: behavior matches default resolution and worktree creation (**`resolve_default_integration_base_ref`**, **`fetch_integration_base`**, worktree from that tip). **`changeset.yaml`** records **`effective_worktree_integration_base_ref`**; **`worktree_integration_base_ref`** is omitted.
  - With **`Some(ref)`**: the worktree branch starts at the tip of that ref after fetch; **`changeset.yaml`** records both **`effective_worktree_integration_base_ref`** and **`worktree_integration_base_ref`**.

### Resume

- **`resolve_persisted_worktree_integration_base_for_session(session_dir, repo_root)`** returns the persisted effective ref if present, else the persisted user chain ref, else the default-resolved base. Downstream code that recreates or validates worktrees for resume should use this helper so sessions that used a chain base keep the same effective base.

### Limitations

- Entry points that call only **`setup_worktree_for_session`** (no optional argument) do not pass a chain base; product wiring must thread **`setup_worktree_for_session_with_optional_chain_base`** where chain PR selection exists.
- **`setup_worktree_for_session_with_integration_base`** (explicit single-segment ref) does not write **`effective_worktree_integration_base_ref`** / **`worktree_integration_base_ref`**; observability for that path differs from the optional-chain API unless aligned in a follow-up.

## Call-site behavior

**`setup_worktree_for_session(repo_root, session_dir)`** resolves the integration base inside **tddy-core** using the default remote branch rules above. Registry helpers (**`effective_integration_base_ref_for_project`**) apply when a caller has loaded **`ProjectData`**; those callers pass the explicit ref into **`setup_worktree_for_session_with_integration_base`**. Layers that only supply repository root and session directory rely on default resolution.
