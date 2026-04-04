# Changesets Applied

Wrapped changeset history for tddy-tools.

- **2026-04-04** [Bug Fix] Embeds **`analyze`** schema — **`get-schema`** / **`submit`** for bugfix **`analyze`**; **`goals.json`**-driven build alongside existing workflow schemas. See [docs/ft/coder/workflow-json-schemas.md](../../../../docs/ft/coder/workflow-json-schemas.md). (tddy-tools, tddy-workflow-recipes)
- **2026-03-28** [Feature] Session context CLI — `set-session-context` merges JSON into `.workflow/<id>.session.json` (`TDDY_SESSION_DIR`, `TDDY_WORKFLOW_SESSION_ID`); aligns with `Context::merge_json_object_sync` for `goal_conditions`. See `docs/ft/coder/workflow-json-schemas.md` and this file’s CLI table. (tddy-tools, tddy-core)
- **2026-03-28** [Feature] Workflow JSON Schemas — Embeds schemas from `tddy-workflow-recipes/generated/` via `goals.json`-driven build; CLI `list-schemas`, manifest module `schema_manifest`, common-schema load fail-fast with cache, stdin size cap. See `docs/ft/coder/workflow-json-schemas.md` and `packages/tddy-tools/docs/json-schema.md`. (tddy-tools, tddy-workflow-recipes)
- **2026-03-22** [Feature] Toolcall submit immediate acknowledgment — Relay writes `SubmitOk` before presenter scheduling; integration test `submit_relay_no_poll` (dev-dependency on `tddy-core` for `start_toolcall_listener` only in tests). (tddy-tools, tddy-core)
- **2026-03-22** [Feature] Production-only red logging markers — `red.schema.json`: optional `source_file` on each `markers[]` item; parity with `packages/tddy-core/schemas/red.schema.json`; schema validation tests cover `source_file`. (tddy-tools, tddy-core)
- **2026-03-13** [Bug Fix] Session and Workflow Fixes — Permission routing via TDDY_SOCKET, tool_in_repo_pre_allowed, non-blocking relay. path.is_some_and(Self::path_allowed). (tddy-tools)
- **2026-03-10** [Feature] tddy-tools Relay Handler — Renamed from tddy-permission. CLI with `submit`, `ask` subcommands and `--mcp` mode. Relays agent tool calls to tddy-coder presenter via Unix domain socket IPC. Local JSON schema validation before relay. Blocking ask for clarification questions. (tddy-tools)
- **2026-03-07** [Feature] Permission Handling in Claude Code Print Mode — New MCP server crate with approval_prompt tool. stdio transport. Denies unexpected requests (TTY IPC deferred). Used by Claude Code --permission-prompt-tool. (tddy-permission)
