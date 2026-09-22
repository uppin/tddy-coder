# tddy-session-store

Session storage on disk: atomic file writes, session directories, the workflow error type, and
declarative session actions (YAML manifests, validation, invocation on the action runtime).

This code used to live in `tddy-core`. It **does not know about the workflow engine, backends or
the presenter**, and must not start to. Its only workspace dependencies are:

| Crate | Why |
|---|---|
| `tddy-workflow` | `ClarificationQuestion` for `WorkflowError`'s clarification variant, and the session artifact layout `output` writes into |
| `tddy-actions` | `session_actions/runtime.rs` runs every manifest on the action runtime |
| `tddy-task` | the task registry those runs are tracked in |

None of the three depends on `tddy-core`, so none can close a cycle.
`packages/tddy-core/tests/session_store_shape.rs` asserts against any other workspace dependency.

## Module layout

| Module | Owns |
|---|---|
| `atomic_file` | `write_atomic` / `write_atomic_labelled`: swap file, `fsync`, then `rename`, so a full disk never truncates session state |
| `error` | `BackendError`, `ParseError`, `WorkflowError` |
| `output` | session directory creation and naming, session file reads and writes, `default_tddy_data_dir` |
| `session_actions` | action manifests: parsing, validation (`jsonschema`), discovery and listing, invocation on the action runtime, test-summary extraction |

## What stayed in `tddy-core`

`tddy_core::{atomic_file, error, output, session_actions}` re-export this crate with `pub use …::*;`,
so **no public path changed**. `tddy_core::session_actions` also keeps `list_actions_in_session_dir`
and `invoke_action_in_session_dir`, which find the repo root through the session's `changeset.yaml`
(`read_changeset`). The changeset belongs to the workflow layer, which this crate must not depend on.

Log targets still read `tddy_core::session_actions::…`. They are part of the observable logging
configuration, so the move left them unchanged.

Write new code against `tddy_session_store` directly.
