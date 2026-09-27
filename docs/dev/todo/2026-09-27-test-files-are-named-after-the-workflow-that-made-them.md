# 2026-09-27 — 374 test files are named after the workflow or a category that partitions nothing

**Category:** Future enhancement (naming debt; the rule is now written down, the backlog is not cleared)
**Source:** Review feedback during the subagent input-validation / output-bounds / provider-queue
change, which had itself added ten `*_red.rs` and `*_acceptance.rs` files before the rule existed.
Those ten are renamed (the last three pending, see below); everything below predates it.

## The rule

Written down in three places, all saying the same thing:

- [`docs/dev/guides/testing.md`](../guides/testing.md) § Workflow Names in Test or File Names
- `.agents/commands/red.md` § Names describe the subject, never the ritual
- `.agents/skills/fluent-tests/references/generic-guidelines.md` § File names follow the same rule
- `.cursor/rules/testing-practices.mdc` (the always-loaded summary)

A test file is named for **what it tests**. Two things are excluded:

1. **The workflow that produced it** — `_red`, `_green`, `_tdd`, `_wip`, a PR number, a changeset
   slug, "phase 2". Every test here was once red, so `_red` distinguishes nothing; it records
   which command the author typed. `red.md` already banned this in test *descriptions*; the file
   name is the same violation one level out, where it is more visible and harder to grep away.
2. **`acceptance`** — which looks like a level marker but is not one. It names no point on the
   unit/integration/e2e scale, and the 348 files carrying it are overwhelmingly ordinary
   integration tests, so it partitions nothing and only crowds out the words that would have said
   what the file covers.

**`unit`, `integration` and `e2e` are legitimate and stay.** They state the scope and cost of the
file — what it touches, how slow it is, what a failure implicates — which is real information a
reader wants before opening it, and which stays true for the file's whole life. So are the
suffixes a runner discovers by: `.test.ts`, `.test.tsx`, `.cy.ts`, `.cy.tsx`.

`_test` / `_tests` on a file already inside `tests/` is redundant rather than wrong; drop it when
renaming anyway, but it is not what this entry is about.

## The backlog

Measured 2026-09-27, `packages/` only, after this change's own ten renames:

| Pattern | Files | In scope |
|---|---|---|
| `*_acceptance.rs` | 348 | yes — not a level marker |
| `*_red.rs` | 26 | yes — names the workflow |
| `*_integration*.rs` | 27 | **no** — legitimate level marker |
| `*_unit_tests.rs` | 14 | **no** — legitimate level marker |
| `*_test.rs` / `*_tests.rs` | 30 | only as redundancy, not as a violation |
| **Total in scope** | **374** | |

```bash
find packages -name '*_acceptance.rs' -o -name '*_red.rs' | wc -l
```

The 149 `.test.ts(x)` and 257 `.cy.ts(x)` files are **not** in scope: those extensions are how
Vitest and Cypress find them.

Most of the 348 will become either a bare subject name or a subject name plus `integration`,
whichever the file actually is — which means the sweep is not purely mechanical: somebody has to
decide, per file, whether the marker is earned. That is the main reason this is not a one-liner.

## Why it was deferred

A rename is cheap per file and expensive in aggregate: 445 of them churn `git log --follow` and
`git blame` across most of the test suite, and every one collides with any open branch touching
that file. This repo runs large refactors as long-lived background stacks, so at any moment a
meaningful share of those files are being edited somewhere else.

It was also not the change's subject. Folding a 445-file rename into a change about subagent
validation and provider admission would have buried a reviewable diff under a mechanical one — the
same reasoning `/pr-wrap`'s file-length gate applies to decompositions.

## What would close it

- **Not one commit.** Per package, or per pattern, so each is separately reviewable and separately
  revertable, and so a conflict with an in-flight branch is contained.
- **Check the stacks first.** `gh pr list --state open` and skip packages a stack is mid-way
  through; those cost a conflict in every node.
- **A rename only** — no content edits in the same commit, or the diff stops being scannable.
- **Fix the cross-references.** Test files cite each other by name in their doc comments
  (`read_window_engine_red.rs` and `codebase_access_red.rs` are both cited from files that are not
  being renamed with them), so a rename that does not sweep the prose leaves dangling pointers.
- **Per file, decide whether the marker is earned.** An `*_acceptance.rs` that is really an
  integration test becomes `<subject>_integration.rs`; one that is a plain unit test loses the
  suffix entirely. This is a judgement per file, not a `sed`, and it is what makes the sweep
  slower than its size suggests.

## What this change already did

Ten files renamed, and the rule written into the four documents above. The last three land as soon
as the provider-queue implementation in flight stops referencing them by name:

| Was | Now |
|---|---|
| `subagent_tool_argument_validation_red.rs` | `subagent_tool_argument_validation.rs` |
| `subagent_system_prompt_override_red.rs` | `subagent_system_prompt_override.rs` |
| `subagent_tool_call_arguments_red.rs` | `subagent_tool_call_arguments.rs` |
| `subagent_generation_cap_red.rs` | `subagent_generation_cap.rs` |
| `subagent_search_result_cap_red.rs` | `subagent_search_result_cap.rs` |
| `search_window_engine_red.rs` | `search_window_engine.rs` |
| `subagent_system_prompt_mcp_acceptance.rs` | `subagent_system_prompt_mcp_surface.rs` |
| `provider_queue_red.rs` | `provider_queue.rs` |
| `subagent_provider_queue_visibility_red.rs` | `subagent_provider_queue_visibility.rs` |
| `subagent_provider_admission_red.rs` | `subagent_provider_admission.rs` |
