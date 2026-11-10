# Changeset: Codex usage from its rollout files

**Date**: 2026-11-10
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 3 — one engine crate plus spec bindings; integration tests over rollout fixtures; no proto, no I/O seam beyond files
**Branch**: `feature/usage-binding/codex-rollout`, base `feature/usage-binding/live-tap` (#892) — `#usage-binding 4/5`

## Initial Discovery

[2026-11-10-usage-binding-codex-rollout-initial-discovery.md](./2026-11-10-usage-binding-codex-rollout-initial-discovery.md)

## Affected Packages

- **tddy-usage**: the `jsonl` format gains five features:
  - `aggregate: last`;
  - header identity (`identity: { id, parent }`);
  - children by parent field;
  - a root match (`main.match`, newest-wins on file modification time, files with a parent excluded);
  - counter expressions with `-` and `+` (`CounterExpr`).

  Each feature is validated at load.
- **tddy-coding-agent-specs**: bindings in `codex.yaml` and `codex-acp.yaml`, plus the ACP cost
  bindings if `#usage-binding 3/5` left them for `last` (see its *Boundaries*).

## Related Feature Documentation

- [PRD-2026-11-10-usage-binding-codex-rollout.md](../../ft/coder/1-WIP/PRD-2026-11-10-usage-binding-codex-rollout.md)
- [session-token-accounting.md](../../ft/coder/session-token-accounting.md)

## Summary

Codex threads, and the threads they spawn, are accounted from Codex's rollout files through YAML
bindings. The `jsonl` format gains the features the binding needs. Codex is not named anywhere in Rust.

## Background

The rollout shape, **verified on this machine (Codex CLI 0.167, 2026-11-10)**:
- **The header:** the first line is `{"type":"session_meta","payload":{"id":<thread>,
  "parent_thread_id":<parent>?, "cwd":…, "source":…}}`. A spawned thread (`thread_source:
  guardian_review`, `source.subagent`) carries `payload.parent_thread_id` **at the top level of the
  payload**, not under `source.subagent.thread_spawn` as the planning brief assumed, and it **shares
  its parent's `cwd`**.
- **The usage:** `{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{
  "input_tokens":58823019,"cached_input_tokens":57833728,"cache_write_input_tokens":0,
  "output_tokens":208721,"reasoning_output_tokens":58680,"total_tokens":59031740}}}}`. This is
  **cumulative**, and `cached_input_tokens` is a subset of `input_tokens`.
- **The model:** `turn_context.payload.model`.

## Responsibility

This PR owns:

- **In `tddy-usage`**: `Aggregate::Last`, `IdentitySpec`, `ChildrenSpec::ByParentField`, `MainMatch`,
  `CounterExpr` (parse and evaluate), and their validation.
- **The Codex bindings**.

## Boundaries

This PR does not:

- change the record, the stream or the web;
- add the `sqlite` format (`#usage-binding 5/5`);
- mint or record the Codex thread id. The root is found by `cwd`.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `#usage-binding 1/5` #890 `claude-transcript` | the `jsonl` format, `RecordSpec`, `FieldPath`, `PathTemplate`, the tree, the record | extends `RecordSpec.aggregate`, `ChildrenSpec`, `MainSpec`, `CounterMap` values | change `sum`/dedupe semantics or the Claude binding |
| `#usage-binding 3/5` #892 `live-tap` (branch line only) | `merge_sources`, the tap | — | — |

## Draft PR contract

The first push (commit 2) publishes the new enum variants and structs, parsed and validated, with
evaluation bodies `// TODO(codex-rollout): implement`, plus the Codex bindings and the failing tests.

## Green wave

**Wave:** 2 of 2
**Greenable independently:** after #890 is green. Every test evaluates the engine over fixtures.
**Concurrent with:** `#usage-binding 2/5`, `3/5`, `5/5`.
**Blocks:** none.

    n1 → n2, n3, n4, n5

## Scope

- [ ] `aggregate: last`
- [ ] Header identity and children by parent field, recursive
- [ ] The root match by field, excluding files that have a parent, newest wins
- [ ] Counter expressions
- [ ] Codex bindings, verified against real rollout files
- [ ] Docs: `session-token-accounting.md`, the `coding-agent-specs.md` usage field table

## Technical Changes

### State A (Current)

The Codex driver debug-logs its usage (`stream/codex.rs:65`), and no rollout file is read.
`codex.yaml` has no `home:` (session default) and `placements: [host]`, so Codex runs with the OS
user's home, and its rollouts land in `~/.codex/sessions/YYYY/MM/DD/`.

### State B (Target)

```yaml
usage:
  sources:
    - format: jsonl
      main:
        path: "{agent_home}/.codex/sessions/*/*/*/rollout-*.jsonl"
        match: { field: payload.cwd, equals: "{worktree}" }
      identity: { id: payload.id, parent: payload.parent_thread_id }
      children: { by_parent_field: true }
      record:
        where: [{ field: payload.type, equals: token_count }]
        aggregate: last
        model: payload.model
        tokens:
          input: "payload.info.total_token_usage.input_tokens - payload.info.total_token_usage.cached_input_tokens"
          cache_read: payload.info.total_token_usage.cached_input_tokens
          cache_write: payload.info.total_token_usage.cache_write_input_tokens
          output: "payload.info.total_token_usage.output_tokens - payload.info.total_token_usage.reasoning_output_tokens"
          reasoning: payload.info.total_token_usage.reasoning_output_tokens
```

- **`identity`** is read from the first line where the field exists. `main.match` is compared against
  that header. Files whose identity has a `parent` are never roots, and the newest root wins.
- **Children by parent field.** Every file in the main glob whose identity `parent` equals a
  conversation's id is a child of it, recursively, with the same depth and cycle guards as 1/5.
- **The model is a field the filter excludes.** `record.model` reads the model from every line, not
  only the kept ones, so `turn_context` lines supply it. The model rule becomes "the last model seen
  in the file". This is a refinement of 1/5's rule, and it is compatible with Claude, whose model sits
  on the kept lines.
- **`CounterExpr`** is `path (('+'|'-') path)*`. A missing operand is 0, and a negative result
  saturates at 0 and logs a warning.
- **Whether Codex's `output_tokens` includes reasoning** is verified against the fixtures (OpenAI's
  documented semantics) before the subtraction is kept.

### Delta (What's Changing)

See *Affected Packages*.

## Implementation Milestones

- [ ] Each engine feature, with unit tests
- [ ] The bindings
- [ ] Fixtures cut from real rollouts (content stripped; `session_meta` header, `turn_context`, `token_count` only)

## Testing Plan

Integration tests in `tddy-usage` evaluate the real `codex.yaml` binding over fixtures under a tempdir
home. Unit tests cover the expression parser and the guards.

## Acceptance Tests

### tddy-usage — `packages/tddy-usage/tests/codex_binding_acceptance.rs`
- `a_thread_reports_its_last_cumulative_token_count`.
- `fresh_input_is_total_input_less_the_cached_part`.
- `reasoning_is_reported_apart_from_output`.
- `a_spawned_thread_is_a_child_of_the_thread_that_spawned_it`.
- `the_main_thread_is_the_newest_root_rollout_in_the_sessions_worktree`.
- `a_child_sharing_its_parents_cwd_is_never_taken_as_the_main_thread`.

### tddy-coding-agent-specs — `packages/tddy-coding-agent-specs/tests/usage_section_acceptance.rs` (extended)
- `a_malformed_counter_expression_is_refused_at_load`.
- `children_by_parent_field_without_an_identity_is_refused_at_load`.

### Unit tests — `packages/tddy-usage/src/counter_expr.rs`
- Parses, evaluates, handles a missing operand, saturates.

## Technical Debt & Production Readiness

(Populated during development.)

## Decisions & Trade-offs

- **The root by `cwd`, not by thread id.** The daemon does not mint Codex's thread id, and Codex keeps
  one conversation per worktree resume. Children are excluded because they share the `cwd`.
- **`main.path` is a three-level date glob, not `**`.** That bounds the directory walk, and
  `{agent_home}/.codex/sessions` holds every thread the user ever ran. A follow-up may narrow it by the
  session's start date (TODO if the walk measures slow).

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

- [x] Record initial discovery (`2026-11-10-usage-binding-codex-rollout-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
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
