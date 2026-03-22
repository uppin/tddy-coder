# Development changesets (cross-package)

Wrapped changesets that span multiple packages or don’t map to a single `packages/*/docs/changesets.md` only.

- **2026-03-22** [Feature] Toolcall submit immediate wire acknowledgment — Unix relay returns `SubmitOk` to `tddy-tools` before presenter `poll_tool_calls`; `SubmitActivity` for activity log; `try_send` with queue-full logging. Package docs updated in `tddy-core` (architecture), `tddy-tools`, `tddy-coder` changesets. (tddy-core, tddy-tools, tddy-coder)
- **2026-03-21** [Feature] Project concept — Proto (`ConnectionService` projects + `project_id` on sessions), `tddy-core` `SessionMetadata.project_id`, `tddy-daemon` project storage + clone-as-user + worker protocol, `tddy-coder` `--project-id`, `tddy-web` Connection screen accordions. (tddy-service, tddy-core, tddy-daemon, tddy-coder, tddy-web)
