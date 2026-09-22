# tddy-session-store architecture

## Overview

Session storage on disk: atomic file writes, session directories, the workflow error type and
declarative session actions. Nothing here knows about the workflow engine, the backends or the
presenter. The session-aware layer that needs those stays in
[`tddy-core`](../../tddy-core/docs/architecture.md), which re-exports every module of this crate at
its old path with `pub use tddy_session_store::<module>::*;`.

### Dependency rule

The crate's only workspace dependencies are these three. None of them depends on `tddy-core`, so
none can close a cycle:

| Crate | Why |
|---|---|
| `tddy-workflow` | `ClarificationQuestion`, for `WorkflowError`'s clarification variant, and the session artifact layout `output` writes into |
| `tddy-actions` | `session_actions::runtime` runs every manifest on the action runtime |
| `tddy-task` | the task registry those runs are tracked in |

`packages/tddy-core/tests/session_store_shape.rs` pins this. It parses the declared dependency
tables into crate names, handling dotted keys, `[dependencies.<name>]` headers, `package =` renames,
`target.*` tables and comments, and matches **exact** names against that three-crate allowlist.
Exact matching matters because a prefix match on `tddy-workflow` would also admit
`tddy-workflow-recipes`, which depends on `tddy-core`. The suite also asserts that no allowlisted
crate reaches `tddy-core`, whether directly or through another workspace crate.

### What stays in `tddy-core`

- **`tddy_core::{atomic_file, error, output}`** are pure glob facades.
- **`tddy_core::session_actions`** is a facade that also defines something. It re-exports this
  crate's `session_actions` and keeps `session_dir.rs`, with `list_actions_in_session_dir`,
  `invoke_action_in_session_dir` and `ListActionsResponse`. Those find the repo root through the
  session's `changeset.yaml` (`read_changeset`, matching `WorkflowError::ChangesetMissing`). The
  changeset belongs to the workflow layer, which this crate must not depend on.
- **`tddy_core::session_action_jobs`** (the async job runner) stays for the same reason, and reaches
  across the crate boundary into `session_actions::runtime`. See
  [Runtime](#runtime-session_actionsruntime--dochidden-not-api).

Log targets inside the moved code still read `tddy_core::session_actions::…`. They are part of the
observable logging configuration, so existing log filters keep matching.

New code should name `tddy_session_store::…` directly.

## Modules

### Atomic file writes (`atomic_file`)

The single way session and daemon state reaches disk. `std::fs::write` opens the target with
`O_TRUNC`, discarding the previous contents before the first replacement byte lands. A write that
fails part-way (`ENOSPC` mid-write, or at writeback after a short `write`) therefore leaves a
**truncated or empty** file where the state used to be. A 0-byte `.session.yaml` does not read as
"write failed". It reads as "not a session", so the daemon and `tddy-web` drop a session whose agent
process is still running and healthy.

- **write_atomic(path, contents)**: writes a **per-call** swap file beside the target
  (`.<basename>.<pid>.<uuid>.swap`), then `write_all` + `sync_all`. It carries over an existing
  target's permission bits, `rename`s over the target, then best-effort `fsync`s the directory.
  Missing parent directories are created. Any failure is cleaned up and reported with the target
  untouched. The swap file is created exclusively, so a path this call did not create is never
  opened or removed. The swap name is per-call because two concurrent writers sharing one fixed
  scratch name can publish each other's half-written bytes.
- **write_atomic_with_mode(path, contents, mode)**: the same, with the replacement's Unix
  permission bits fixed by the caller and applied when the swap file is created. `write_atomic`
  can only carry over bits that already exist, so a **first** write lands at the process umask.
  Secrets use this variant so their bytes are never on disk at a wider mode, not even briefly.
  `tddy-daemon-auth`'s `github_token_store` and `tddy-screen-sharing`'s vault write through it.
  On platforms without Unix permissions it behaves as `write_atomic`.
- **write_atomic_labelled(path, contents)**: `write_atomic` with the target path folded into the
  error string, since a bare `ENOSPC` names no file.

Consumers reach it as `tddy_core::atomic_file` or `tddy_session_store::atomic_file`: everything
that persists session or daemon state. That covers, in `tddy-core`, `session_metadata`,
`session_context`, `changeset`, `workflow/{session,action_cache}`, `session_action_jobs/runner`,
`backend/{codex,cursor}` and `presenter`, and in this crate `output/writer.rs` and
`session_actions/runtime.rs`. Outside both it covers `tddy-workflow-recipes`, `tddy-projects`,
`tddy-worktree-service`, `tddy-session-lifecycle`, `tddy-telegram`, `tddy-host-service`,
`tddy-daemon-auth`, `tddy-screen-sharing` and `tddy-sandbox-recipes`.

These writes deliberately do **not** go through it:

- `/proc`, `/sys` and cgroup control files. These are kernel interfaces, where a rename is
  meaningless and a plain write is the contract.
- Ready markers, empty completion markers and short-lived curl body files. Nothing reads them after
  a failed write.
- `tddy-build`'s action cache. The crate is deliberately standalone (no `tddy-*` dependencies), and
  it already writes a uniquely named temp file, syncs it and renames it.
- `tddy-tool-engine`'s writes. They target arbitrary repository files, where replacing a symlink
  with a regular file would change behaviour.

Atomic replacement changes the file's **inode**. Nothing in the repo watches session files through
`notify`/inotify (`usage_watcher.rs` documents polling as a deliberate choice), so no reader depends
on the inode surviving. Swap files are dotfiles ending in `.swap`, so directory scans that filter on
`.md` (`inject_cross_references`) or skip hidden files never pick one up.

### Errors (`error`)

`BackendError`, `WorkflowError` and `ParseError`, the error vocabulary every layer above shares.
The module's one import from elsewhere in the workspace is `tddy_workflow::ClarificationQuestion`,
which serves `WorkflowError::ClarificationNeeded { questions, session_id }`.

### Session directories (`output`)

Goal-agnostic session directory and path helpers. Structured-response parsing and the TDD artifact
writers live in `tddy-workflow-recipes`, not here.

- **create_session_dir_with_id(base, id)**: creates `{base}/sessions/{id}/`
  (`SESSIONS_SUBDIR` = `sessions`). **create_session_dir_in(base)** does the same with a fresh
  UUIDv7 id. **create_session_dir_under** is an alias kept for call sites that once treated `base`
  as the sessions folder; the contract is always `{base}/sessions/<id>/`.
- **default_tddy_data_dir** / **default_tddy_data_dir_for(debug)**: the profile-aware data-dir
  default. A debug build uses the repo-local `tmp/.tddy`, and a release build returns `None`, so
  callers resolve the per-user `$HOME/.tddy`.
- **write_session_file** / **read_session_file** (`.session`), **write_impl_session_file** /
  **read_impl_session_file** (`.impl-session`): the session's recorded backend session ids,
  written through `write_atomic_labelled`.
- **plan_artifacts_root(session_dir)**: `<session_dir>/artifacts/`, delegating to
  `tddy_workflow::session_artifacts_root`.
- **inject_cross_references(content, session_dir, self_name)**: appends a "Related Documents"
  section linking the directory's other `.md` files.
- **slugify_directory_name**: `YYYY-MM-DD-<slug>` directory names.

The session tree itself is described in
[session-layout.md](../../../docs/ft/coder/session-layout.md).

### Session actions (`session_actions`)

Library support for declarative **`actions/*.yaml`** manifests (see
[session-actions.md](../../../docs/ft/coder/session-actions.md)). **`tddy-tools`** wires
**`list-actions`**, **`invoke-action`** and their JSON output on top of this module.

- **ActionManifest**: versioned YAML (**`serde`**, **`deny_unknown_fields`**).
  **`parse_action_manifest_file`** / **`parse_action_manifest_yaml`** load a single manifest.
- **list_action_summaries**: discovers, filters and pages manifests across two roots. The first is
  the per-repo store, **`<tddy_data_dir>/actions/<repo_key>/`**. The second is the session overlay,
  **`<session_dir>/actions/`**. Overlay entries win when both roots share a relative path, and
  manifests that fail to parse are skipped with a warning. **`derive_repo_key`** /
  **`repo_actions_root`** compute the store root.
- **resolve_action_manifest_path**: resolves an action id to its manifest across the same roots.
  A mismatch surfaces as **`UnknownActionId`**, which callers map to tool exit semantics
  (**`classify_session_actions_exit_code`**).
- **validate_action_arguments_json**: compiles **`input_schema`** when present and validates the
  **`--data`** JSON (**`jsonschema`**) before any subprocess runs.
- **validate_authored_manifest**: field checks on a subagent-*authored* manifest before it is
  established as an action file: non-empty argv, a filename-safe **`id`** (letters, digits, `-` and
  `_` only) and a compilable **`input_schema`**. Both the in-jail `request_action` retry loop
  (**`tddy-tools`**) and the host-side `EstablishAction` handler (**`tddy-sandbox-app`**) call it,
  so the two cannot drift (see [no-bash-mode.md](../../../docs/ft/coder/no-bash-mode.md)).
- **`authoring`** (**`author_prompt`**, **`extract_manifest_yaml`**, **`prevalidate_manifest_yaml`**,
  **`MAX_AUTHOR_ATTEMPTS`**, **`MAX_MANIFEST_BYTES`**): the rules an authored manifest must satisfy,
  and the retry guidance returned when it does not. The rules live here, not in the crate that
  advertises `request_action` or the one that establishes it, so those two cannot drift.
- **resolve_allowlisted_path**: resolves **`output_path_arg`** string fields inside the canonical
  session directory, or inside the optional repo root. Traversal outside those roots fails closed.
- **ensure_action_architecture**: enforces **`architecture`** (**`native`**, or a rustc-style
  prefix match on **`std::env::consts::ARCH`**) before spawn.
- **invoke_action_core**: the whole invocation, shared by the `tddy-tools` CLI's local path and the
  `TDDY_SOCKET` relay listener. It resolves the manifest, validates arguments, checks the output
  binding and architecture, then calls **run_manifest_command**. That function runs the manifest's
  **`command`** verbatim on the action runtime, in the repo root when there is one and the session
  directory otherwise. **`finalize_invocation_record`** merges a **`summary`** object into the
  returned JSON when **`result_kind`** is **`test_summary`**.
- **parse_test_summary_from_process_output** / **`TestSummary`**: parse cargo-style
  **`test result:`** totals from combined stdout/stderr.
- **`tool_gate`** (**`SESSION_ACTION_TOOLS_ENV`** = `TDDY_SESSION_ACTION_TOOLS`,
  **`session_action_tools_enabled`**): whether this session's host actually serves the three
  session-action tools. `request_action`, `list_actions` and `invoke_action` are host round-trips
  against a session directory that exists only on the host, and **the transport cannot say whether
  they are served**. The in-jail socket carries both `tddy-sandbox-app`'s handler, which implements
  all three, and `tddy-daemon`'s, which implements none and answered
  `{"error":"unknown tool: ListActions","is_error":true}` to every call. So the **host declares
  it**, the same way it declares an available language server with `TDDY_LSP_TOOLS`. Only a host
  that routes the three somewhere that answers them sets the variable. Every other placement stays
  silent, and the tools are not advertised. The gate lives in the crate that owns session actions,
  because that is the only one both the advertiser and the implementer already name.

#### Runtime (`session_actions::runtime`) — `#[doc(hidden)]`, not API

Every manifest runs as a task on `tddy-actions`' `ProcessRuntime`, tracked in a per-session
`tddy_task::TaskRegistry`. The re-exported entry points are **`action_manifest_to_spec`** and
**`run_manifest_blocking`**.

The module itself is `#[doc(hidden)] pub mod runtime`. It is public only because
`tddy_core::session_action_jobs::runner` stays in `tddy-core` and reaches it across the crate
boundary. `runner` needs `read_changeset`, which is why it stays. That exposes seven functions,
also through the facade glob as `tddy_core::session_actions::runtime::…`:

- `session_task_registry`
- `start_manifest_async`
- `schedule_async_log_mirror`
- `task_status_for_job`
- `cancel_task_in_registry`
- `block_on`
- `write_channel_logs`

None of it is API. **`block_on` panics when called from inside a current-thread runtime**: it uses
`block_in_place` when a runtime is already current, and falls back to a private multi-thread
runtime otherwise.

## Tests

The in-module unit tests moved with the code (`atomic_file`, `output/writer.rs`,
`session_actions/{authoring,tool_gate}.rs`). The integration suite stays in `tddy-core`, where
**`session_actions_acceptance`** exercises the code through the `tddy_core::session_actions` facade. The crate's dependency shape is pinned by
**`tddy-core/tests/session_store_shape.rs`**.
