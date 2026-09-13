# JSON Schema embedding and validation (`tddy-workflow-recipes`)

## Purpose

This crate embeds its own workflow JSON Schemas from **`generated/`** and validates structured agent
output before it is relayed to **`tddy-coder`**. Goal names and schema filenames come from
**`goals.json`** in this package, and this package's **`build.rs`** writes
**`generated/goal_registry.rs`** with `GOAL_SCHEMA_FILES` — so the registry, the schemas and the code
that reads them are all in one crate, with no cross-package `include_dir!` or `include_str!` reach.

See [workflow-schemas.md](./workflow-schemas.md) for `goals.json`'s own structure, the proto
registry, and what the build script validates.

## Modules

| Module | Responsibility |
|--------|----------------|
| `schema` | Embedded tree, `get_schema`, `validate_output`, common resource registration for `$ref`, `write_schema_to_path` |
| `schema_manifest` | Parses embedded `schema-manifest.json` for `list_registered_goals()` |
| `github_pr` | The GitHub pull-request REST client, over `github_rest_common` |

## The CLI that serves them

The subcommands below are **`tddy-tools`'** clap surface — that crate parses the arguments, prints
the JSON contract and owns the exit codes; it calls into `schema` and `schema_manifest` for
everything about schemas.

| Subcommand | Behavior |
|------------|----------|
| `submit` | `--goal` + `--data` / `--data-stdin`. `--goal` is **authoritative for routing**, falling back to the payload's `goal` field; validates when the resolved goal has a registered schema. A `--goal` disagreeing with the payload's `goal`, or a submission naming no goal in either place, is a **usage error (exit 2)** — never an `ok` acknowledgement under the goal `unknown` |
| `get-schema <goal>` | Prints schema JSON; `-o` writes goal file and `common/` subtree |
| `list-schemas` | Prints `{"goals":[...]}` |
| `ask` | Clarification relay (separate JSON schema) |
| `set-session-context` | Merges JSON into `.workflow/<id>.session.json` (`TDDY_SESSION_DIR`, `TDDY_WORKFLOW_SESSION_ID`); not listed in `goals.json` |
| `persist-changeset-workflow` | `--session-dir`, `--data` — validates JSON against **`changeset-workflow`**, writes **`workflow`** on **`changeset.yaml`** atomically; listed in `goals.json` for schema embedding |

### MCP mode (`tddy-tools --mcp`)

The permission-prompt MCP server registers this crate's GitHub REST helpers as
**`github_create_pull_request`** and **`github_update_pull_request`** when **`GITHUB_TOKEN`** or
**`GH_TOKEN`** is set. **`ServerInfo`** instructions list those tool names alongside the base
permission contract so agents discover them without relying on implicit tool lists. The `#[tool]`
advertisement and its input schemas live in `tddy-tools`, which is the crate that speaks MCP; the
REST calls behind them are `github_pr` here. Recipe alignment (merge-pr, **tdd-small**) is in
**`docs/ft/coder/github-pr-tools-mcp.md`**.

### `branch-review` and `review.md`

For goal **`branch-review`**, after JSON Schema validation succeeds, **`submit`** writes
**`review.md`** under **`TDDY_SESSION_DIR`** when that environment variable is present (agent
subprocesses set it). When the variable is absent, validation and relay behavior are unchanged; the
file write is skipped. The write itself is
`tddy_workflow_recipes::review::persist_review_md_to_session_dir`.

## Logging

`env_logger` initializes in the binary at startup (default level **warn**; use `RUST_LOG` for
`info` / `debug`).

⚠ **The `log` targets these modules emit name `tddy_tools`, not this crate**: `tddy_tools::schema`
(6 sites), `tddy_tools::schema_manifest` (3) and `tddy_tools::github_pr` (13). A log target is an
operator's `RUST_LOG` filter, so these are the strings that select these modules' output today and
the ones to use. Whether they should be renamed to match the crate — and whether the workspace's
rule is "the target names where the code is" or "the target never changes, because renaming
silently breaks a filter and the failure mode is missing logs" — is an open question, recorded in
[`docs/dev/todo/2026-09-10-schema-validation-still-logs-under-the-tddy-tools-target-after-moving-crates.md`](../../../docs/dev/todo/2026-09-10-schema-validation-still-logs-under-the-tddy-tools-target-after-moving-crates.md).
Do not change them without settling that.

## Testing

**`tests/schema_validation_tests.rs`** in this package asserts parity between source
`schemas/red.schema.json` and `generated/red.schema.json`, and covers the validation fixtures.
Integration tests under `packages/tddy-tools/tests/` cover the CLI behaviour on top of it, driving
the built binary.

## Related packages

- **`tddy-tools`** — the binary: argument parsing and dispatch, the permission-decision engine, and
  the MCP router that advertises the GitHub PR tools
- **`tddy-coder`** — where validated output is relayed
