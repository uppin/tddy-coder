# Changeset: PR-stack documents (per-PR PRD + changeset, attached to the child)

**Date**: 2026-08-29
**Status**: 🚧 In Progress
**Type**: Feature
**Branch**: `feat-attach-pr-stack-docs`

## Affected Packages

- **tddy-workflow-recipes**: [README.md](../../packages/tddy-workflow-recipes/README.md)
  - `pr_stack/docs.rs` (new) — the `write-stack-docs` payload types, validator, writer and path helper
  - `pr_stack/mod.rs` — the fourth goal, its graph edge and hints, the state table, `STATE_STACK_DOCS_WRITTEN`
  - `pr_stack/hooks.rs` — the artifact-path correction, `after_write_stack_docs`, the two new prompts, the refinement state reset
  - [changesets.md](../../packages/tddy-workflow-recipes/docs/changesets.md) — changeset index entry
- **tddy-coder**: [README.md](../../packages/tddy-coder/README.md)
  - the stack-seeding path — writes `STATE_STACK_DOCS_WRITTEN` where it wrote `STATE_STACK_PLANNED`
  - [changesets.md](../../packages/tddy-coder/docs/changesets.md) — changeset index entry
- **tddy-daemon**: [README.md](../../packages/tddy-daemon/README.md)
  - `stack_doc_attachments.rs` (new) — builds a node's `SessionAttachment` list from the orchestrator
  - `connection_service.rs` — `StackChildSpawnHandler::spawn_child` attaches; the child prompt names the changeset
  - `session_context_docs.rs` — per-PR document rows; `relative_path` on every row
  - `session_list_enrichment.rs` — populates the new proto field
  - [changesets.md](../../packages/tddy-daemon/docs/changesets.md) — changeset index entry
- **tddy-service**: [README.md](../../packages/tddy-service/README.md)
  - `connection.proto` — `SessionContextDoc.relative_path` (field 8)
  - [changesets.md](../../packages/tddy-service/docs/changesets.md) — changeset index entry
- **tddy-web**: [README.md](../../packages/tddy-web/README.md)
  - `CreateSessionPane.tsx`, `useSessionAttachments.ts` — initial attachment rows
  - `prstack/stackDocAttachments.ts` (new) — the web mirror of the daemon's attachment list
  - `prstack/PrStackScreen.tsx` — populates them for a planned-PR start
  - `attachments/contextDocPath.ts` (new) — one `relative_path` derivation for both consumers
  - `attachments/HostDocumentPicker.tsx` — reads `relativePath` instead of deriving it
  - [changesets.md](../../packages/tddy-web/docs/changesets.md) — changeset index entry

## Related Feature Documentation

- [PRD: PR-stack documents](../ft/coder/pr-stack-docs.md)
- [PR stacking](../ft/coder/pr-stacking.md) — amended: pipeline, state table, seeded-stack state, artifact paths
- [Session attachments](../ft/coder/session-attachments.md) — reused verbatim
- [Exploration artifact](../ft/coder/exploration-artifact.md) — reused verbatim

## Checklist

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Write acceptance tests
- [x] Write unit tests
- [x] Implement M0 — artifact path correction
- [x] Implement M1 — the `write-stack-docs` goal
- [x] Implement M2 — attach on the agent spawn path
- [x] Implement M2b — the changeset prompt line reaches both spawn paths
- [x] Implement M3 — attach in the Start-session dialog
- [x] Implement M4 — list per-PR documents
- [ ] Update `docs/ft/coder/pr-stacking.md`

## State A — current behaviour

Verified in this branch, which has no diff against `master`.

| Fact | Evidence |
|---|---|
| A child session receives only `title` + `description` | `PrStackScreen.tsx:254`; `connection_service.rs:6913-6917` |
| `StackNode` has no document field | `changeset.rs:53-89` — 11 fields |
| The context header is session-scoped, so a fresh child gets none | `workflow/mod.rs:306`; a new child's `artifacts/` is empty |
| `stack-plan.yaml` and `pr-stack-plan.md` are written to the **session root** | `pr_stack/hooks.rs:333,337`; pinned by the test at `:566-583` |
| …while every reader but one resolves manifest basenames under `artifacts/` | `session_context_docs.rs:76`, `:167`; `host_documents.rs:105` |
| …the exception being the context header's legacy fallback | `workflow/mod.rs:262-276` |
| `SESSION_ARTIFACT` already resolves **nested** relative paths | `host_documents.rs:211` joins the full path; `:40` permits separators; `:200-204` validates only the filename |
| Attachment destinations are flat, one level | [session-attachments.md](../ft/coder/session-attachments.md); `artifact_paths.rs:31-47` |
| Cross-host attachment materialization exists | `materialize_host_document_attachment`; `HostDocumentRef.daemon_instance_id`; 4 MiB cap at `host_documents.rs:30` |
| Only `tdd` has generated goal schemas | `generated/` contains `proto_basenames.rs`, `schema-manifest.json`, `tdd/` |
| `write-stack-plan` parses a raw YAML submit in its hook | `hooks.rs:319-322`, `serde_yaml::from_str::<StackPlanOutput>` |
| `GithubPrApi::create_pr` has no `draft` parameter | `orchestrate_pr_stack/github.rs:97-103` |
| Drafts are read correctly and recorded as `open` | `github.rs:68`; `pr_stack/mod.rs:1397-1399` |
| `context_docs_for_session` already mixes a static manifest with a dynamic scan | `session_context_docs.rs:72-110` |
| `HostDocumentPicker` reconstructs a relative path from `kind` + `basename` | `HostDocumentPicker.tsx:100-113` |

## Milestones

### M0 — Artifact path correction *(prerequisite)*

`stack-plan.yaml` and `pr-stack-plan.md` move under `session_artifacts_root(dir)`. Without this,
attaching `pr-stack-plan.md` is impossible: a `SESSION_ARTIFACT` ref to it cannot resolve.

Existing on-disk orchestrators need no migration — `build_context_header` already falls back to the
session root, so their agent keeps seeing the files while new writes land in the right place.

### M1 — The `write-stack-docs` goal

Graph gains `write-stack-plan --GoTo--> write-stack-docs --GoTo--> orchestrate`. New state
`StackDocsWritten` sits between `StackPlanned` and the interactive loop; `StackPlanned` now routes to
`write-stack-docs`.

`pr_stack/docs.rs` holds the payload types and the validator:

```rust
pub struct StackDocsOutput { pub version: u32, pub docs: Vec<NodeDocs> }
pub struct NodeDocs { pub node_id: String, pub prd: String, pub changeset: String }

pub const REQUIRED_CHANGESET_HEADINGS: &[&str] = &[
    "## Responsibility",
    "## Boundaries",
    "## Dependencies",
    "## Draft PR contract",
];

pub fn node_doc_paths(session_dir: &Path, node_id: &str) -> NodeDocPaths;
pub fn validate_stack_docs(stack: &Stack, out: &StackDocsOutput) -> Result<(), String>;
```

`tddy-coder`'s seeding path writes `STATE_STACK_DOCS_WRITTEN`, preserving "a seeded orchestrator
comes up in `orchestrate`".

### M2 — Attach on the agent spawn path

`stack_doc_attachments.rs` builds the four-item list for a node; `spawn_child` passes it to
`spawn_claude_cli_session_inner` and appends a line to `initial_prompt` naming the attached
changeset. A document that does not exist is skipped.

### M2b — One prompt-line rule, both spawn paths

A dialog-started child was attached the documents and never told to read them: the line was appended
in `spawn_child` alone, and `start_session_core` serves the dialog. The rule now lives in one place
(`prompt_with_attached_changeset`) and is applied at every attachment-materialization seam that goes
on to launch an agent — the claude-cli branch, the cursor-cli branch and `spawn_split_agent` — rather
than being restated in TypeScript, where a second copy could only drift.

Two things keep it narrow. `prepare_session_attachments` now answers with the attachments that
**actually reached** the session's store, so the line can only name a document the child holds. And
the changeset is recognised by its **source** — a `SESSION_ARTIFACT` ref reading
`prs/<node_id>/changeset.md` — not by its destination basename: `start_session_core` serves every
`StartSession`, so an operator who attaches their own file called `changeset.md` must not be told to
read boundaries nobody wrote, and a renamed row must still be recognised.

### M3 — Attach in the Start-session dialog

`CreateSessionInitialValues` gains `attachments`; `useSessionAttachments` accepts initial rows;
`PrStackScreen` populates them. Rows render as ordinary, removable attachment rows.

### M4 — List per-PR documents

`context_docs_for_session` scans `artifacts/prs/*/` and emits `MANIFEST` rows.
`SessionContextDoc.relative_path` (field 8) is populated for every row and consumed by
`HostDocumentPicker` in place of its `kind`-based derivation.

## Existing tests the state-table change supersedes

The pipeline gains a goal and the state table re-points `StackPlanned`, so tests written against the
three-goal pipeline assert something the PRD deliberately replaces. They are **updated, not deleted**
— each keeps its original intent, which the change does not touch:

| Test | Was | Now |
|---|---|---|
| `pr_stack/mod.rs` `resuming_a_planned_stack_continues_into_the_docs_pass` | `StackPlanned → orchestrate` | `StackPlanned → write-stack-docs` (renamed) |
| `pr_stack/mod.rs` `every_non_terminal_state_resumes_into_the_orchestrate_loop` | case `StackPlanned` | case `StackDocsWritten` |
| `pr_stack/mod.rs` `graph_flows_plan_phase_into_a_terminal_orchestrate_goal` | edge `write-stack-plan → orchestrate` | both edges through `write-stack-docs` |
| `tests/pr_stack_free_prompting_acceptance.rs` `planning_flows_into_a_terminal_orchestrate_loop` | edge `write-stack-plan → orchestrate` | both edges through `write-stack-docs` |
| `tests/pr_stack_free_prompting_acceptance.rs` `resuming_a_documented_stack_drops_into_the_orchestrate_loop` | `StackPlanned` | `StackDocsWritten` (renamed) |
| `packages/tddy-coder/tests/pr_stack_seed_start_acceptance.rs` (×2) | — | **unchanged**; they already pinned "a seeded orchestrator comes up in `orchestrate`", which the one-token `STATE_STACK_DOCS_WRITTEN` change in `run.rs` restores |

The "orchestrate is terminal, there is no auto-loop" assertion that both graph tests exist to make is
untouched — only the goal preceding it moved.

## A known-weak test, kept and labelled

`a_documented_stack_resolves_to_the_orchestrate_goal` passes **vacuously**: `next_goal_for_state`
ends in a catch-all routing every unrecognised state to `orchestrate`, so it would pass whether or
not `StackDocsWritten` is named in the table. It is kept because it still catches a *wrong explicit*
mapping, and carries a doc comment saying so. The load-bearing half of the pair is
`a_planned_stack_resolves_to_the_docs_goal`, which the catch-all cannot satisfy.

Removing the catch-all to make it meaningful was considered and rejected: it is what lets a legacy
persisted state (`assess`, `wait`, and the states of the removed auto-loop) resume into the operator
loop instead of dead-ending.

## Design decisions

### The manifest stays static; per-PR rows are discovered by scanning

`known_artifacts()` returns `&[(&'static str, &'static str)]` and cannot enumerate N per-node files.
Rather than making the manifest dynamic — which would ripple into every recipe implementing the
trait — per-PR rows join the list the same way attachment rows already do: a directory scan at
enrichment time. The static manifest keeps describing the stack-level artifacts alone.

### Documents are addressed by convention, not recorded on the node

`node_doc_paths(session_dir, node_id)` derives both paths. Nothing is added to `StackNode`,
`PlannedPr`, `AddPlannedPrRequest`, `PrAddPlannedInput` or `WireStackNode`. `write-stack-docs` writes
every node in one pass, so paths are predictable by construction. The cautionary precedent is
`AddPlannedPrInput.child_recipe` — accepted, validated, then silently dropped, because `StackNode`
had nowhere to put it (`pr_stack/mod.rs:600-610`). A field is additive if a node ever needs to point
elsewhere.

### Required headings are validated structurally; their content is not

Presence of `## Responsibility`, `## Boundaries`, `## Dependencies` and `## Draft PR contract` is
mechanically checkable and enforced. Whether the boundaries are *correct* is not, for the same
reason [`validate_stack_plan` does not enforce the PR boundary
contract](../ft/coder/pr-stacking.md#pr-boundary-contract-every-node-is-self-contained): judging a
vertical slice needs the diff the node has yet to produce, and any check collapses into a keyword
heuristic that is trivially reworded around. The split is deliberate — structure is cheap to verify,
semantics stay prompt-carried and human-reviewed.

### A partial docs pass is refused

A submit omitting any planned node is rejected and writes nothing. A node with no boundaries
document is exactly the duplicate-development hazard the feature exists to prevent, and a
half-written pass would leave the operator unable to tell "not written yet" from "deliberately
empty".

### A plan refinement returns the session to the docs pass

`after_write_stack_plan` already re-seeds the stack on every refinement turn; it now also sets
`StackPlanned`, which routes to `write-stack-docs`. Refining the plan therefore regenerates all
documents before the operator returns to `orchestrate`. This interrupts the chat on purpose:
documents that silently describe a superseded plan are worse than absent ones, because an agent
reading them believes them. `write-stack-docs` is idempotent and rewrites every node.

### A seeded stack skips the docs pass

Seeding binds one node to a session whose work already exists and may be underway, so a retroactive
PRD documents a decision nobody is about to make. The seeding path starts at `StackDocsWritten`,
which keeps the documented behaviour that a seeded orchestrator resumes straight into `orchestrate`
and avoids the trap that `reseed_stack_from_plan_if_unspawned` would reject its plan anyway.

### A missing document is skipped at spawn, not fatal

Starting a node before the docs pass has run is sometimes correct. Failing the spawn would make the
docs pass a hard prerequisite for all work; the operator is told which documents were unavailable
instead.

### One helper feeds both spawn paths

The agent's `pr_spawn_child` and the web dialog build their attachment list from the same function.
A child that differs by how it was started is a bug the operator cannot see.

## Files to create

| File | Purpose | State |
|---|---|---|
| `packages/tddy-workflow-recipes/src/pr_stack/docs.rs` | payload types, `validate_stack_docs`, `write_stack_docs`, `node_doc_paths` | signatures + unit tests; bodies `todo!()` |
| `packages/tddy-daemon/src/stack_doc_attachments.rs` | builds a node's `SessionAttachment` list, and the child prompt line | signatures + unit tests; bodies `todo!()` |
| `packages/tddy-web/src/components/sessions/prstack/stackDocAttachments.ts` | the same list for the Start-session dialog, derived from the orchestrator's `context_docs` | ✅ |
| `packages/tddy-web/src/components/sessions/attachments/contextDocPath.ts` | `relative_path` with the legacy `kind` derivation as its fallback, shared by the picker and the dialog | ✅ |
| `docs/ft/coder/pr-stack-docs.md` | PRD | ✅ |

The two new modules are created in the red phase as **signatures with `todo!()` bodies**, each marked
`TODO(pr-stack-docs)`. Without them the acceptance tests fail to *compile* rather than to *assert*,
which hides a typo in a test behind a missing API and gives the green phase no API to fill in. Their
public shape is the contract the tests pin; the bodies are `/green`'s work.

## Files to modify

| File | Change |
|---|---|
| `packages/tddy-workflow-recipes/src/pr_stack/mod.rs` | fourth goal + edge + hints; `STATE_STACK_DOCS_WRITTEN`; state table; `goal_requires_tddy_tools_submit` |
| `packages/tddy-workflow-recipes/src/pr_stack/hooks.rs` | M0 path fix; `after_write_stack_docs`; two prompts; refinement state reset |
| `packages/tddy-coder` stack-seeding path | write `STATE_STACK_DOCS_WRITTEN` |
| `packages/tddy-daemon/src/connection_service.rs` | `spawn_child` attaches; prompt names the changeset |
| `packages/tddy-daemon/src/session_context_docs.rs` | per-PR rows; `relative_path` on every row |
| `packages/tddy-daemon/src/session_list_enrichment.rs` | populate `relative_path` |
| `packages/tddy-service/proto/connection.proto` | `SessionContextDoc.relative_path = 8`; correct the stale `:2119` comment |
| `packages/tddy-web/src/components/sessions/CreateSessionPane.tsx` | `attachments` in `CreateSessionInitialValues` |
| `packages/tddy-web/src/hooks/useSessionAttachments.ts` | accept initial rows |
| `packages/tddy-web/src/components/sessions/attachments/pendingAttachment.ts` | `InitialAttachment` — an attach row before the form assigns its identity |
| `packages/tddy-web/src/components/sessions/prstack/PrStackScreen.tsx` | populate attachments for a planned-PR start |
| `packages/tddy-web/src/components/sessions/attachments/HostDocumentPicker.tsx` | read `relativePath` |
| `docs/ft/coder/pr-stacking.md` | pipeline, state table, seeded state, artifact paths |

## Acceptance tests

### M0 — `packages/tddy-workflow-recipes/tests/pr_stack_artifact_paths_acceptance.rs`

1. **`write_stack_plan_persists_the_plan_and_its_markdown_under_the_artifacts_root`** — both files
   land in `artifacts/`, and neither remains at the session root. Replaces the current
   `write_stack_plan_still_persists_stack_plan_yaml_and_md_alongside_exploration`
   (`hooks.rs:566-583`), which asserts the opposite.
2. **`a_plan_left_at_the_legacy_session_root_is_still_advertised_to_the_agent`** — an orchestrator
   whose files predate the move still gets them in its `<context-reminder>`, pinning the fallback
   that makes the move migration-free.
3. **`a_stack_plan_under_the_artifacts_root_is_readable_as_a_host_document`** — the ref that M2
   depends on resolves. This is the test that would have caught the bug.

### M1 — `packages/tddy-workflow-recipes/tests/pr_stack_docs_acceptance.rs`

4. **`write_stack_docs_persists_a_prd_and_a_changeset_for_every_planned_node`** — a two-node stack
   yields exactly `prs/n1/{PRD.md,changeset.md}` and `prs/n2/{PRD.md,changeset.md}`.
5. **`writing_stack_docs_moves_the_session_into_the_orchestrate_loop`** — state becomes
   `StackDocsWritten` and its goal is `orchestrate`.
6. **`a_refined_plan_returns_the_session_to_the_docs_pass`** — after a refinement submit, state is
   `StackPlanned` and its goal is `write-stack-docs`.
7. **`a_seeded_stack_reaches_the_orchestrate_loop_without_a_docs_pass`** — a stack seeded from an
   existing session starts at `StackDocsWritten`.
8. **`rewriting_stack_docs_replaces_the_previous_documents`** — idempotence: a second submit with
   changed bodies overwrites rather than appending.

### M1 validation — `packages/tddy-workflow-recipes/tests/pr_stack_docs_validation_acceptance.rs`

9. **`a_submit_naming_a_node_outside_the_stack_is_refused_and_writes_nothing`**
10. **`a_submit_omitting_a_planned_node_is_refused_and_writes_nothing`**
11. **`a_changeset_without_a_draft_pr_contract_heading_is_refused`**
12. **`a_changeset_without_a_dependencies_heading_is_refused`**
13. **`a_blank_prd_is_refused`**
14. **`a_rejected_submit_leaves_the_previously_written_documents_untouched`** — the "writes nothing"
    guarantee holds against existing files, not just an empty directory.

### M1 prompts — added to `pr_stack/hooks.rs` beside `pr_boundary_scoping_rule_tests`

15. **`the_docs_prompt_requires_each_dependency_to_name_what_that_pr_delivers`** — drives the real
    `before_task` seam, matching how the boundary contract is pinned rather than asserting on string
    constants.
16. **`the_docs_prompt_requires_a_draft_pr_contract_of_api_plus_failing_tests`**

### M2 — `packages/tddy-daemon/tests/pr_stack_child_doc_attachment_acceptance.rs` (10)

These pin the pure helper `stack_doc_attachments`, **not** the `spawn_child` wiring — see the coverage
note below.

17. **`a_documented_node_attaches_its_own_pair_and_the_two_shared_documents`** — exactly four, in
    listing order.
18. **`a_nodes_own_documents_are_read_from_its_directory`** — nested source (`prs/n2/PRD.md`), flat
    destination (`PRD.md`).
19. **`the_shared_documents_are_read_from_the_artifacts_root`**
20. **`every_document_is_read_from_the_orchestrators_session_artifacts`** — `SESSION_ARTIFACT`, the
    orchestrator's `session_id`.
21. **`every_document_is_read_from_the_host_that_owns_the_orchestrator`** — the ref names the
    orchestrator's `daemon_instance_id`, not the spawning daemon's. Name the wrong host and the fetch
    reads an empty artifacts directory on the wrong machine.
22. **`every_destination_is_a_flat_basename`** — a separator is refused by the attachment store.
23. **`a_node_started_before_the_docs_pass_attaches_only_the_shared_documents`** — missing documents
    are skipped, not fatal.
24. **`an_orchestrator_with_no_exploration_map_attaches_the_documents_it_has`**
25. **`an_orchestrator_with_nothing_written_attaches_no_documents`** — an empty list, not a refusal.
    *The one test here that would also pass against a stub returning `vec![]`; the other nine compare
    whole vectors or exact paths and cannot.*
26. **`a_node_without_documents_does_not_borrow_another_nodes`** — attaching n1's boundaries to n2 is
    worse than attaching none.

**The `spawn_child` wiring is covered by M2b's tests below**, not by these. All ten above, and the
unit tests beside them, exercise the pure helper: delete the wiring and every one still passes while
the agent spawn path silently attaches nothing.

*(An earlier draft of this list named `the_child_prompt_names_the_attached_changeset` and
`an_attached_document_does_not_displace_the_childs_own_prd`. Neither was written: the prompt line is
covered by the `stack_doc_attachments.rs` unit test
`the_prompt_line_names_the_attached_changeset_by_path`, and PRD/attachment coexistence is a property
of `write_attachment_bytes` that this feature does not change.)*

### M2b — `connection_service.rs` § `stack_child_spawn_tests` (4) + `stack_doc_attachments.rs` (5)

The spawn tests live **in the crate** because `StackChildSpawnHandler` is private and constructing
one is the narrowest way to drive the real wiring; publishing it to reach it from `tests/` would
widen the surface for a test's convenience. Each drives a real spawn — a git worktree is cut, the
daemon's own materializer runs, and a stub in place of `claude` records the command line it was
handed, read off disk rather than off the PTY (a terminal capture wraps at the window width, which
would split the sentence under test).

27. **`a_child_the_agent_spawned_holds_the_nodes_documents`** — the four documents land in the
    child's `artifacts/attachments/`, and its changeset is its own node's, byte for byte.
28. **`a_child_the_agent_spawned_is_told_to_read_its_changeset`**
29. **`a_child_started_from_the_dialog_is_told_to_read_its_changeset`** — the gap M2b closes: same
    documents, same line, through `start_session_core`.
30. **`a_child_started_before_its_documents_were_written_is_told_nothing`** — the shared pair still
    arrives; nothing points at a changeset nobody wrote.

Beside the rule, on the source-not-destination distinction:

31. **`a_file_the_operator_uploaded_is_not_taken_for_the_nodes_changeset`** — staged bytes named
    `changeset.md`.
32. **`a_changeset_outside_a_nodes_documents_directory_is_not_the_nodes`** — a `SESSION_ARTIFACT`
    ref that is not under `prs/`.
33. **`a_nodes_changeset_is_recognised_by_where_it_is_read_from`** — a renamed row is still the
    node's changeset, and the line names where it actually landed.
34. **`a_childs_prompt_gains_the_line_naming_its_changeset`**
35. **`a_childs_prompt_is_untouched_when_no_changeset_was_attached`**

### M3 — `packages/tddy-web/cypress/component/PrStackStartSessionDocAttachmentsAcceptance.cy.tsx`

Uses `mountWithRpc` + `anInMemoryRpcBackend`.

23. **`lists the node's PRD and changeset alongside the shared stack documents`**
24. **`attaches the changeset that belongs to the node being started`** — the sent ref reads
    `prs/n2/changeset.md`, never a sibling node's. Asserted on the request rather than the row: the
    row shows the flat destination name, and the source path exists nowhere in the DOM.
25. **`lists only the shared documents for a node the docs pass has not covered`**
26. **`lists no documents for an orchestrator that has written none`** — and the dialog still starts.
27. **`removes an auto-attached document when the operator drops it`**
28. **`sends exactly the documents left attached when the session is started`**
29. **`sends each document as a reference to the orchestrator's own session`**

### M4 — `packages/tddy-daemon/tests/pr_stack_context_docs_acceptance.rs`

30. **`context_docs_lists_each_planned_prs_prd_and_changeset`**
31. **`a_per_pr_document_carries_its_nested_relative_path`** — `prs/n1/PRD.md`, not `PRD.md`.
32. **`a_manifest_document_carries_its_own_basename_as_its_relative_path`**
33. **`an_attachment_carries_its_attachments_prefixed_relative_path`** — the derivation the picker
    used to do server-side now.
34. **`an_orchestrator_with_no_docs_pass_lists_only_its_stack_level_artifacts`**

### M4 web — `packages/tddy-web/cypress/component/HostDocumentPickerNestedDocAcceptance.cy.tsx`

35. **`the host document picker offers a nested per-PR document`**

## Unit tests

### `pr_stack/docs.rs`

- `node_doc_paths_resolves_both_documents_under_the_nodes_directory`
- `validate_stack_docs_accepts_a_complete_pass_over_a_two_node_stack`
- `validate_stack_docs_names_the_missing_node_in_its_error`
- `validate_stack_docs_names_the_missing_heading_in_its_error`
- `required_headings_are_matched_at_the_start_of_a_line` — `## Boundaries` in prose does not satisfy
  the check
- `a_heading_with_trailing_whitespace_satisfies_the_check`

### `pr_stack/mod.rs`

- `stack_planned_resolves_to_the_docs_goal`
- `stack_docs_written_resolves_to_the_orchestrate_goal`
- `stack_docs_written_reports_an_active_status`
- `write_stack_docs_requires_a_tddy_tools_submit`
- `a_legacy_init_state_with_a_populated_stack_still_resumes_into_orchestrate` — regression guard on
  `next_goal_for_state_with_changeset`

### `stack_doc_attachments.rs`

- `a_nodes_attachment_list_names_four_documents_when_all_exist`
- `a_missing_per_pr_document_is_omitted_from_the_list`
- `every_attachment_targets_the_session_artifact_scope`
- `destination_basenames_are_flat`

### `session_context_docs.rs`

- `per_pr_rows_follow_the_stack_level_rows`
- `a_node_directory_holding_an_unexpected_file_is_ignored` — only `PRD.md` / `changeset.md` are
  surfaced, so a stray file cannot masquerade as a document

## Out-of-scope ideas

Logged to `docs/dev/TODO.md` under **Future Enhancements**, source `pr-stack-docs`:

- **A `pr_write_node_docs` tool** regenerating one node's documents instead of the whole stack.
- **An RPC exposing `read_session_context_doc_utf8`**, and the Docs tab that would consume it — the
  follow-up `pr-stacking.md:353` already anticipates.
- **Attach-after-start**, so a running child can receive documents written after it spawned.
- **`stack-progress.json` is documented but unimplemented** — the PRD describes it as a host
  guarantee; no production code writes it, and the orchestrator syncs through the child's
  `changeset.yaml` instead. Either implement or correct the document.
- **`GrillMeRecipe` ships blank context-doc descriptions** — it does not override
  `artifact_doc_descriptions()`, unlike `PrStackRecipe`.
- **The `stack-plan-md` submit key is vestigial** — `write_stack_plan_system_prompt` asks the agent
  for it, but `after_write_stack_plan` ignores it and generates the markdown itself.
