# tddy-session-actions architecture

## Overview

Session actions run against **one session**: listing and invoking them from a session directory,
the background jobs that run them, and the pipeline that validates and chains their results. Each
of these reads the session's `changeset.yaml`, which is why they sit **above**
[`tddy-changeset`](../../tddy-changeset/docs/architecture.md). The declarative action store itself
lives in [`tddy-session-store`](../../tddy-session-store/docs/architecture.md), beneath both.

Workspace dependencies: `tddy-changeset`, `tddy-session-store`, `tddy-task`. `jsonschema` is a
dependency here because of `session_action_pipeline`'s output transforms.

`tddy-core` re-exports this crate whole (`pub use tddy_session_actions::*;`), so every `tddy_core::{session_actions, session_action_jobs, session_action_pipeline}::…` path consumers name resolves unchanged. New code should name `tddy_session_actions` directly.
Log targets keep their `tddy_core::session_actions::…`, `tddy_core::session_action_jobs` and
`tddy_core::session_action_pipeline` names, so existing log filters keep matching.

## Session actions (`session_actions`)

- **Glob over the store**: `pub use tddy_session_store::session_actions::*;`. Manifest parsing,
  discovery, validation, authoring rules, invocation, test-summary parsing and the `tool_gate` all
  live in
  [`tddy-session-store`](../../tddy-session-store/docs/architecture.md#session-actions-session_actions),
  and every `tddy_core::session_actions::…` path resolves through this module.
- **`session_dir`**: **`list_actions_in_session_dir`**,
  **`invoke_action_in_session_dir`** and **`ListActionsResponse`** list and invoke a session's
  actions from a session directory alone. They take the repo root from the session's
  `changeset.yaml` through **`read_changeset`** (matching **`WorkflowError::ChangesetMissing`**),
  and the action store from the tddy data directory. The result has the same JSON shape a relayed
  `list-actions` answers with. `tddy_tools::session_actions_cli` wraps them in the CLI's argument
  parsing, stdout and exit codes. Logs go under **`tddy_core::session_actions::session_dir`**. This
  file cannot live in the store: it reads the changeset, and `tddy-changeset` itself depends on
  `tddy-session-store`.
- **Tests**: **`session_actions_acceptance`**.

## Session action jobs (`session_action_jobs/`)

- **Purpose**: Optional **non-blocking** runs of the same declarative **`actions/*.yaml`** manifests, keyed by a **`job_id`**, with filesystem **stdout** / **stderr** capture paths and **`wait` / `stop`** operations. Shares manifest resolution (**`resolve_action_manifest_path`**), argument validation, **`repo_path`** / **`output_path_arg`** checks, and **`ensure_action_architecture`** with the synchronous **`invoke-action`** path. See [session-actions.md](../../../docs/ft/coder/session-actions.md) (**Session action jobs** section).
- **On-disk layout**: **`<session_dir>/session_action_jobs/jobs/<job_id>/`** holds **`job.json`**, **`stdout.log`**, **`stderr.log`**. **`SessionActionJobRegistry::load`** creates **`<session_dir>/session_action_jobs/`** and **`jobs/`**.
- **invoke_session_action**: With **`async_start: false`**, blocks until the subprocess exits and returns the same structured record shape as **`invoke-action`** (including **`test_summary`** when configured). With **`async_start: true`**, admits the job, creates log files before return, spawns the manifest command in a new process group on Unix, assigns a version-7 **UUID** as **`job_id`**, and returns **`running`** status plus absolute capture paths.
- **wait_session_action_job**: Polls subprocess exit via **`waitpid`** (**`WNOHANG`**) while **`job.json`** reflects **`running`**; **`timeout_ms: None`** or **`0`** means unbounded wait; a positive bound yields **`TimedOut { still_running }`** when the deadline elapses first.
- **stop_session_action_job**: Sends **`SIGKILL`** to the process group on Unix, reaps the child, persists **`cancelled`** state, returns **`UnknownJob`** when the job directory is absent, and **`AlreadyFinished`** when the job is already terminal. **`stable_code`** on **`SessionActionJobsError::UnknownJob`** is **`unknown_job`**.
- **Platform notes**: Async **`wait` / `stop` / `reap`** use **`libc`** on Unix targets; non-Unix builds surface **`JobState`** errors for those entry points.
- **Runtime access**: the runner drives manifests through `tddy_session_store::session_actions::runtime` (`block_on`, `write_channel_logs`, the per-session task registry). That module is `#[doc(hidden)] pub` only so this runner can cross the crate boundary. It is not API. The runner lives here, not in the store, because it needs `read_changeset`.
- **Tests**: **`toolcall_jobs`** (this crate), **`session_action_jobs_acceptance`** (tddy-tools).

## Session action pipeline (`session_action_pipeline`)

- **Purpose**: Helpers for env merge (override precedence), canonical **`args`/`env`** JSON value, glob resolution relative to a base path, channel manifests (**`stdout`**, **`stderr`**, **`logs`**), optional **input mapper** and **output transform** subprocesses with JSON Schema validation on transform output, and **primary** spawn with explicit argv and capture files. Complements **`session_actions`**; see [session-actions.md](../../../docs/ft/coder/session-actions.md) (**Session action pipeline** section).
- **Subprocess contract**: Mapper and transform children receive **`TDDY_SESSION_CHANNEL_MANIFEST_JSON`**. Mapper stdin receives caller JSON; stdout must be a single JSON object with **only** **`args`** and **`env`**. Primary and subprocess paths use **`env_clear`** then caller-supplied **`envs`** (plus the manifest variable where set).
- **Dependencies**: **`glob`**, **`jsonschema`**, **`serde_json`**, **`log`**.
- **Tests**: **`session_action_resolve_unit`** (this crate), **`session_action_pipeline_integration`** (tddy-tools).
