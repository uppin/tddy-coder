# Changeset: OpenCode usage from its SQLite store

**Date**: 2026-11-10
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 3 — one engine crate plus a spec binding; a SQLite I/O seam (read-only, lock-tolerant); integration tests over a fixture database
**Branch**: `feature/usage-binding/opencode-sqlite`, base `feature/usage-binding/codex-rollout` (#893) — `#usage-binding 5/5`, PR [#894](https://github.com/uppin/tddy-coder/pull/894), stack [#895](https://github.com/uppin/tddy-coder/stack/895)

## Initial Discovery

[2026-11-10-usage-binding-opencode-sqlite-initial-discovery.md](./2026-11-10-usage-binding-opencode-sqlite-initial-discovery.md)

## Affected Packages

- **tddy-usage**: the `sqlite` format (`src/sqlite.rs`): `SqliteSource`, read-only evaluation,
  identifier validation, the root match, recursive children by parent column. It adds
  `rusqlite = { version = "0.32", features = ["bundled"] }`, the version `tddy-semantic-index` already
  uses, so **no new external dependency**.
- **tddy-coding-agent-specs**: the binding in `opencode.yaml`, and validation of the `sqlite` section.

## Related Feature Documentation

- [PRD-2026-11-10-usage-binding-opencode-sqlite.md](../../ft/coder/1-WIP/PRD-2026-11-10-usage-binding-opencode-sqlite.md)
- [session-token-accounting.md](../../ft/coder/session-token-accounting.md)

## Summary

OpenCode sessions and the sessions they spawn are accounted from `opencode.db` through a YAML binding
over a new, generic, read-only `sqlite` format.

## Background

The schema, **verified on this machine (2026-11-10)**:
- **Per-session totals** live in `session_v2` (`id`, `parent_id`, `directory`, `agent`, `model`,
  `cost REAL`, `tokens_input`, `tokens_output`, `tokens_reasoning`, `tokens_cache_read`,
  `tokens_cache_write`, `time_created`, `time_updated`).
- **The messages table is now `session_message`.** It holds no `tokens` column, and the per-message
  `message.data` the planning brief described is gone.

Where the store lands:
- **Jailed:** `home: { kind: session }` gives `<session_dir>/sandbox/.work/home`, so the store is under
  `.local/share/opencode/opencode.db` there.
- **Host:** the OS user's home.

OpenCode keeps one conversation per worktree (`--continue`).

## Responsibility

This PR owns:

- **`UsageSource::Sqlite(SqliteSource { path, table, id, parent, root: RootMatch { column, equals,
  newest_by }, tokens: ColumnCounterMap, cost, model, name })`**, its evaluation and its validation.
- **The OpenCode binding**.

## Boundaries

This PR does not:

- write to the database or run SQL the binding does not name;
- change the `jsonl` format, the record, the stream or the web.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `#usage-binding 1/5` #890 `claude-transcript` | `UsageSource` (format-tagged), `UsageContext`, `PathTemplate`, the tree guards, the record | adds the `sqlite` variant; renders `path` and `root.equals` with `PathTemplate` | change the other variants or the record |
| `#usage-binding 4/5` #893 `codex-rollout` (branch line, plus one shared surface) | `CounterExpr` (`a + b`, `a - b`) | reuses it over column names for `tokens` | change the expression grammar |

The `CounterExpr` reuse is a **consumed surface, not behaviour**. If this node is greened before #893,
its tests use only single-column counters, and `reasoning` subtraction waits for #893's surface.
Recorded so `/green` does not reimplement the expressions.

## Draft PR contract

The first push (commit 2) publishes `SqliteSource` and its structs, parsed and validated, with the
evaluation body `// TODO(opencode-sqlite): implement`, plus the OpenCode binding and the failing tests.

## Green wave

**Wave:** 2 of 2
**Greenable independently:** after #890 is green. It needs #893's `CounterExpr` **surface**, which is
published by #893's contract commit and already on this branch, not its behaviour; the tests that
evaluate a subtraction wait for #893's green.
**Concurrent with:** `#usage-binding 2/5`, `3/5`, `4/5`.
**Blocks:** none.

    n1 → n2, n3, n4, n5      (n4 → n5: surface only)

## Scope

- [ ] The `sqlite` format: read-only open, identifier validation, root match, recursive children
- [ ] Lock tolerance: a busy database keeps the last snapshot
- [ ] The OpenCode binding, with the reasoning overlap checked on a real store
- [ ] Docs: `session-token-accounting.md`, the `coding-agent-specs.md` usage field table

## Technical Changes

### State A (Current)

There is no SQLite source in the usage path. OpenCode usage is not accounted.

### State B (Target)

```yaml
usage:
  sources:
    - format: sqlite
      path: "{agent_home}/.local/share/opencode/opencode.db"
      table: session_v2
      id: id
      parent: parent_id
      root: { column: directory, equals: "{worktree}", newest_by: time_updated }
      name: agent
      model: model
      cost: cost
      tokens:
        input: tokens_input
        cache_read: tokens_cache_read
        cache_write: tokens_cache_write
        output: tokens_output
        reasoning: tokens_reasoning
```

- **The connection** is opened with `SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_NO_MUTEX` and a URI with
  `mode=ro`, under a 200 ms busy timeout. `SQLITE_BUSY` or `SQLITE_LOCKED` yields "unavailable", and the
  caller keeps its previous rows. A missing file means no rows.
- **Queries are built only from validated identifiers** (`^[A-Za-z_][A-Za-z0-9_]*$`), each checked at
  load:
  - the root query: `SELECT … FROM <table> WHERE <root.column> = ?1 AND <parent> IS NULL ORDER BY
    <newest_by> DESC LIMIT 1`;
  - the children query: `… WHERE <parent> = ?1 ORDER BY <id>`, applied recursively with the 1/5
    guards.
- **`turns` is not available from `session_v2`.** It reports 0, and the Usage tab already renders 0.
  The alternative, counting `session_message` rows of a `type`, is left out. It would need a second
  query shape, and it is recorded as a TODO.
- **Reasoning overlap.** Whether `tokens_output` already includes `tokens_reasoning` is checked
  against a real store before the binding ships. If it does, `output: "tokens_output -
  tokens_reasoning"`, using #893's `CounterExpr`.

### Delta (What's Changing)

See *Affected Packages*.

## Implementation Milestones

- [ ] `SqliteSource` and validation
- [ ] Read-only evaluation with lock tolerance
- [ ] Recursion
- [ ] The binding, checked against a real store

## Testing Plan

Integration tests in `tddy-usage` build a fixture `opencode.db` with the real `session_v2` DDL (copied
from the store, with rows synthesised) in a tempdir home, and evaluate the real `opencode.yaml`
binding. A lock test holds an `EXCLUSIVE` transaction on a second connection.

## Acceptance Tests

### tddy-usage — `packages/tddy-usage/tests/opencode_binding_acceptance.rs`
- `the_worktrees_newest_root_session_is_the_main_conversation`.
- `a_spawned_session_and_its_own_child_nest_under_their_parents`.
- `tokens_cache_and_cost_come_from_the_session_row`.
- `a_locked_database_reports_unavailable_rather_than_zero`.
- `a_missing_database_reports_no_rows`.

### tddy-coding-agent-specs — `packages/tddy-coding-agent-specs/tests/usage_section_acceptance.rs` (extended)
- `a_sqlite_binding_naming_a_non_identifier_column_is_refused_at_load`.
- `the_builtin_opencode_spec_binds_its_store`.

## Technical Debt & Production Readiness

### Contract commit (pass 2, 2026-11-11)

- **The reasoning overlap is unverified.** The only local `opencode.db` holds no sessions (`select
  count(*) from session_v2` → 0). The binding maps `tokens_output` → `output` and
  `tokens_reasoning` → `reasoning` with no subtraction. **A TODO for `/green` or the sweep**: check a
  real store with a reasoning model, and subtract with `CounterExpr` (`#usage-binding 4/5`) if
  `tokens_output` already includes reasoning.
- **A missing database reports a zero main row**, as a missing Claude transcript does (1/5's rule).
  That supersedes *State B*'s "a missing file means no rows". **A locked or busy database reports no
  rows**, so the caller keeps its last snapshot.
- **The counters are `CounterExpr` over column names.** A row is presented as a JSON object of its
  columns, so 4/5's expressions apply unchanged. `cost`, `model` and `name` are `FieldPath`s of one
  segment, a column name.
- **`rusqlite = { version = "0.32", features = ["bundled"] }`** is now a `tddy-usage` dependency. It is
  the version `tddy-semantic-index` already uses, and `Cargo.lock` gains no new crate version.
- **The fixture DDL is copied from the real `session_v2`** (2026-11-10). The rows are synthesised.

### Red (2026-11-11)

- `opencode_binding_acceptance.rs`: 4 of 5 fail. The locked-store guard holds on the stub.
- `usage_section_acceptance.rs`: the non-identifier refusal fails. `the_builtin_opencode_spec_binds_its_store`
  passes, because the YAML is contract data.

A scoped `./test -p tddy-coding-agent-specs -p tddy-usage` gives 20 suites green and 43 failing
tests, every one a red test of this stack (1/5, 3/5, 4/5 or 5/5). Clippy `-D warnings
--all-targets` is clean.

## Decisions & Trade-offs

- **Declared columns, not SQL.** The binding cannot inject SQL, and the format stays agent-agnostic.
- **`rusqlite`, not `sqlx`.** The read is synchronous and runs on the poller's blocking task, so it
  needs no async pool. It is the same crate and version as `tddy-semantic-index`.

## Refactoring Needed

### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results

### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-11-10-usage-binding-opencode-sqlite-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
