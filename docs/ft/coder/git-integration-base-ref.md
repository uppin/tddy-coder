# Git integration base ref (worktrees)

**Product area:** Coder workflow — git worktrees after plan approval.

## Purpose

Workflow sessions that materialize a git worktree select a **remote-tracking integration base ref** (for example `origin/main` or `origin/master`). That ref is the fetch target and the start point for `git worktree add`.

## Behavior

### The default branch is a property of the project

The remote-tracking integration base ref is a first-class, user-settable property of the
**project** (`main_branch_ref`), managed from the Projects screen
([projects-screen-multi-host.md](../web/projects-screen-multi-host.md#default-branch)). It is the
single source of truth for the default base ref across every session-start surface (web/gRPC
`StartSession` and Telegram).

### Unified resolution

**`effective_integration_base_ref_for_project`** is the one resolver every default-base path
consults when the caller supplies no explicit override:

1. **Default branch set** — when the project row has a `main_branch_ref`, that ref is returned
   verbatim. The probe below **does not run** and has no effect.
2. **Legacy project (no default branch set)** — the ref is resolved live against the project's
   repository via **`resolve_default_integration_base_ref`** after `git fetch origin`:
   1. `origin/master` when that ref exists on the remote.
   2. Otherwise `origin/main` when that ref exists.
   3. Otherwise the symbolic ref `refs/remotes/origin/HEAD` when it points at a valid `origin/<branch>`.

Live probing (`origin/master` → `origin/main` → `origin/HEAD`) is therefore **legacy-only**: it
resolves the default for rows that predate the setting and loses effect the moment a default branch
is stored. This replaces the previous hardcoded `origin/master` fallback for unset rows.

### Project registry

Registered projects live in `~/.tddy/projects/projects.yaml`. Each row has optional
**`main_branch_ref`**: a remote-tracking ref of the form `origin/<path>` (one or more segments,
e.g. `origin/main` or `origin/release/2025`).

- Rows **without** `main_branch_ref` resolve live (legacy path above).
- **`effective_integration_base_ref_for_project`** returns the stored ref, or the live-resolved
  ref for legacy rows.
- **`set_project_default_branch`** updates a project's stored ref (validated before any YAML write);
  **`add_project`** rejects invalid `main_branch_ref` values before any YAML write.

### Validation

A project default branch must be an `origin/<path>` remote-tracking ref with no shell
metacharacters, no `..`, no `--`, and no whitespace — the same class of unsafe input rejected
elsewhere (`validate_chain_pr_integration_base_ref` in **tddy-core**). Any remote branch listed by
`list_recent_remote_branches` is therefore a valid choice, including slash-containing names.
(`validate_integration_base_ref` — single-segment only — still governs the per-session
`selected_integration_base_ref` override.)

## API and tooling surface

- **tddy-daemon** `ProjectData` includes optional `main_branch_ref` (serde).
- **ConnectionService** `ProjectEntry` carries `main_branch_ref` so clients can read a project's
  stored default; empty means "not set" (legacy resolution applies).
- **`SetProjectDefaultBranch`** persists a project's default branch and forwards the change to peer
  hosts that own the same logical `project_id` (logical-project scope, mirroring `AddProjectToHost`).
- **`StartSession`** with an empty `selected_integration_base_ref` resolves its default base through
  `effective_integration_base_ref_for_project`, so the stored default applies to web sessions, not
  only Telegram.

## Related

- [Project concept](../daemon/project-concept.md) — projects registry and session `project_id`.
- **tddy-core** `worktree` module — `fetch_integration_base`, `setup_worktree_for_session_with_integration_base`, `resolve_default_integration_base_ref`, `setup_worktree_for_session`.
- **tddy-daemon** `project_storage` — `effective_integration_base_ref_for_project`, `add_project`.
- **Changeset workflow** — When **`changeset.yaml`** **`workflow`** includes **`branch_worktree_intent`** (**`new_branch_from_base`** or **`work_on_selected_branch`**), worktree creation follows that intent together with **`selected_integration_base_ref`**, **`new_branch_name`**, and **`selected_branch_to_work_on`** as validated by the **`changeset-workflow`** schema. See [Workflow JSON schemas — Changeset workflow](workflow-json-schemas.md#changeset-workflow-persist-changeset-workflow) and [Workflow recipes — TDD](workflow-recipes.md#developer-reference-shipped-recipes).

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

- Entry points that call only **`setup_worktree_for_session`** (no optional argument) do not pass a chain base; product wiring must thread **`setup_worktree_for_session_with_optional_chain_base`** where chain PR selection exists. **Exception (Updated: 2026-04-05):** **`tddy-workflow-recipes`** **`ensure_worktree_for_session`** and **`tddy-service`** session document approval now call **`setup_worktree_for_session_with_optional_chain_base`**, passing **`None`** or the persisted **`changeset.yaml`** **`worktree_integration_base_ref`** so Telegram (and any client that pre-writes that field) can opt into a chain base without a separate CLI flag.
- **`setup_worktree_for_session_with_integration_base`** (explicit single-segment ref) does not write **`effective_worktree_integration_base_ref`** / **`worktree_integration_base_ref`**; observability for that path differs from the optional-chain API unless aligned in a follow-up.

### Telegram inbound (Updated: 2026-04-05)

- After recipe and project selection, **`telegram_session_control`** offers **Default** or a **recent remote branch** list; non-default choices set **`worktree_integration_base_ref`** before **`tddy-coder`** is spawned. See **[telegram-session-control.md](../daemon/telegram-session-control.md)** (**Integration base branch**).

## Session chaining (parent session → `origin/<branch>`)

**tddy-core** exposes **`resolve_chain_integration_base_ref_from_parent_session(sessions_root, parent_session_id, child_project_repo)`**: it reads the parent session directory under **`{sessions_root}/sessions/{parent_session_id}/`**, loads **`changeset.yaml`**, takes the persisted **branch** or **branch suggestion**, builds **`origin/<trimmed-path>`**, validates with **`validate_chain_pr_integration_base_ref`**, and compares canonical **`repo_path`** on the parent changeset with the child project repository when **`repo_path`** is present. When the parent names a branch (or branch suggestion), **`repo_path`** on the parent **`changeset.yaml`** is **required**; without it, resolution fails as **`WorkflowError::ChangesetInvalid`** before repository alignment.

**`integrate_chain_base_into_session_worktree_bootstrap`** validates the resolved ref and calls **`setup_worktree_for_session_with_optional_chain_base(child_repo, child_session_dir, Some(resolved_ref))`** so **`changeset.yaml`** receives **`effective_worktree_integration_base_ref`** and **`worktree_integration_base_ref`** under the same rules as the existing chain-PR path.

Product reference for Telegram ordering and **`tcp:`** wire format: **[telegram-session-control.md](../daemon/telegram-session-control.md)**. Session directory metadata field: **[session-layout.md](session-layout.md)**.

## Call-site behavior

**`setup_worktree_for_session(repo_root, session_dir)`** resolves the integration base inside **tddy-core** using the default remote branch rules above. Registry helpers (**`effective_integration_base_ref_for_project`**) apply when a caller has loaded **`ProjectData`**; those callers pass the explicit ref into **`setup_worktree_for_session_with_integration_base`**. Layers that only supply repository root and session directory rely on default resolution. Workflow hooks that create worktrees after plan approval should prefer **`setup_worktree_for_session_with_optional_chain_base`** when **`changeset.yaml`** may carry **`worktree_integration_base_ref`** (Telegram chain selection, future RPC fields).
