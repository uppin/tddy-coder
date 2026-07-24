# Exploration Artifact (exploration.md) — Feature Document

**Product Area**: Coder
**Status**: Implemented (2026-07-21)
**Updated**: 2026-07-24

## Summary

Workflow planning sessions produce a persisted **`exploration.md`** session artifact that captures
code-discovery knowledge gathered during planning: file and line/column references to relevant code
segments, architecture diagrams (mermaid), documentation references, and conventions/gotchas. Every
workflow step that runs **after the user interview** references `exploration.md` so the fresh agent
started for that step reuses the already-gathered knowledge instead of re-exploring the codebase.
Steps with write access treat it as a **living document** and append newly discovered knowledge as
implementation proceeds.

## Background

Each planning-phase step (interview, plan, acceptance-tests, red) runs as a **fresh agent session
with fresh context** ([planning-step.md](planning-step.md)); implementation steps resume a separate
impl session. Today the only knowledge handed between steps is:

- `PRD.md` (requirements, testing plan, TODO) — inlined into downstream prompts,
- `changeset.yaml` `discovery` (toolchain, scripts, doc locations, coarse `relevant_code`
  path/reason pairs — no line references, no diagrams),
- the `<context-reminder>` header listing existing artifact paths (only for acceptance-tests and
  red today).

The expensive part of planning — locating the exact code segments, understanding module
relationships, finding the authoritative docs — is discarded when the plan agent's session ends.
Every downstream agent re-learns it with its own tool-call budget. The FastContext discovery
subagent ([discovery-agent.md](discovery-agent.md)) produces `path:line-start-line-end` citations,
but they are not persisted as a reusable artifact either.

## Requirements

### R1 — Artifact registration

1. `exploration.md` is a first-class session artifact: canonical path
   `session_dir/artifacts/exploration.md` (same resolution rules as other artifacts).
2. Registered in `known_artifacts()` (key `"exploration"`, basename `"exploration.md"`) for the
   recipes with a planning/discovery phase: **tdd**, **tdd_small**, **bugfix**, **grill-me**,
   **pr-stack** (see R7).
3. Because it is in `known_artifacts()`, it automatically appears in the `<context-reminder>`
   header (absolute path) whenever the file exists.

### R2 — Production by the plan step (tdd, tdd_small)

1. The plan submit schema (`urn:tddy:goal/plan`) gains an **optional `exploration` string
   property**: the full markdown content of the exploration document.
2. The plan agent remains **read-only**; like the PRD, the exploration content is returned inline
   in the submit JSON and the **engine writes** `artifacts/exploration.md` (in `write_artifacts`,
   alongside `PRD.md`). Empty/whitespace-only exploration is treated as absent (no file written);
   a missing field is not an error.
3. The planning system prompt instructs the agent to populate `exploration` with:
   - **Code Map** — file paths with line (and column where meaningful) references to the code
     segments relevant to the feature, each with a one-line "why it matters",
   - **Diagrams** — mermaid diagrams of the relevant module/data flow when structure is non-trivial,
   - **Documentation** — references to authoritative docs (repo docs, package docs, external URLs),
   - **Conventions & Gotchas** — repo-specific conventions and traps discovered while exploring.
4. Plan refinement re-submits may include an updated `exploration`; the engine overwrites the file.

### R3 — Production by the bugfix analyze step

1. The bugfix `analyze` submit schema gains the same optional `exploration` string property, and
   the engine writes `artifacts/exploration.md` from it (analyze is bugfix's discovery step).

### R4 — Consumption by post-interview steps

1. Every post-interview step's prompt is preceded by the `<context-reminder>` header. Today only
   acceptance-tests and red get it; **green, demo, evaluate, validate, refactor, update-docs**
   (tdd) and tdd_small's post-plan steps must get it too, so `exploration.md` (and the other
   artifacts) are advertised to every downstream agent.
2. The step system prompts for **acceptance-tests, red, and green** contain an explicit
   instruction: *before exploring the codebase, read `exploration.md` (when it exists) and reuse
   its knowledge — file/line references, diagrams, documentation pointers — instead of
   re-discovering it*.
3. The interview step is unaffected (it precedes exploration and stays artifact-free).

### R5 — Living document

1. Post-plan steps that have write access (acceptance-tests, red, green) are instructed to
   **append** newly discovered code knowledge to `exploration.md` (new sections or bullets with
   file:line references) when they learn something not already captured.
2. The engine never deletes or truncates an existing `exploration.md` written by a later step;
   plan refinement overwrite (R2.4) applies only while the workflow is still in the plan phase.

### R6 — grill-me

1. The grill-me create-plan step (which has write access and writes brief files directly) is
   instructed via its system prompt to also write `exploration.md` into the session artifacts
   directory with the same content structure as R2.3.

### R7 — pr-stack

1. The `pr-stack` recipe's planning phase (`analyze-stack` → `write-stack-plan`) produces
   `exploration.md`. The `write-stack-plan` submit (`StackPlanOutput`) gains the same optional
   `exploration` string property; the host writes `artifacts/exploration.md` from it (non-blank
   gate, same as R2.2), alongside the existing `stack-plan.yaml` and `pr-stack-plan.md`.
2. `PrStackRecipe`'s manifest registers `("exploration", "exploration.md")`.
3. The interactive `orchestrate` goal is the consuming step: its prompt is preceded by the
   `<context-reminder>` header (`before_task` → `prepend_context_header`), so `exploration.md`
   (and the other on-disk stack artifacts) is advertised to the operator agent. No header is
   injected when no such file exists. See [pr-stacking.md](pr-stacking.md).

## Non-goals

- No structured (JSON/YAML) exploration format — `exploration.md` is markdown for both humans and
  agents. `changeset.yaml` `discovery` is unchanged and remains the machine-readable summary.
- No staleness tracking/invalidation of line references as the code changes (agents are expected
  to treat references as starting points, not guarantees).
- No changes to recipes without a planning/discovery phase (merge-pr, review, free-prompting).
  (pr-stack **is** in scope as of 2026-07-24 — it has an `analyze-stack` → `write-stack-plan`
  planning phase; see R7.)
- No FastContext/discovery-subagent integration in this changeset (see Future Considerations).

## Acceptance Criteria

- [x] Plan submit JSON with an `exploration` field validates against `tddy-tools get-schema plan`;
      omitting the field also validates.
- [x] After the plan step completes with an `exploration` field, `session_dir/artifacts/exploration.md`
      exists with the submitted content.
- [x] After the plan step completes without an `exploration` field (or with a blank one), no
      `exploration.md` is written and the workflow proceeds normally.
- [x] When `artifacts/exploration.md` exists, the acceptance-tests, red, and green prompts start
      with a `<context-reminder>` header listing `exploration.md:` with its absolute path.
- [x] When `exploration.md` does not exist, no `exploration.md:` line appears in the header.
- [x] The acceptance-tests, red, and green system prompts instruct the agent to read
      `exploration.md` before exploring and to append new discoveries to it.
- [x] The planning system prompt describes the `exploration` field with Code Map (file:line refs),
      Diagrams (mermaid), Documentation, and Conventions & Gotchas guidance.
- [x] tdd, tdd_small, bugfix, and grill-me manifests register `("exploration", "exploration.md")`.
- [x] The grill-me create-plan system prompt instructs writing `exploration.md`.
- [x] `StackPlanOutput` accepts an optional `exploration` field (present → `Some`, absent → `None`).
- [x] After pr-stack `write-stack-plan` completes with a non-blank `exploration`,
      `artifacts/exploration.md` is written; a blank/absent field writes no file, and
      `stack-plan.yaml` + `pr-stack-plan.md` are still persisted.
- [x] The pr-stack `PrStackRecipe` manifest registers `("exploration", "exploration.md")`.
- [x] The pr-stack `write-stack-plan` system prompt documents the optional `exploration` field.
- [x] The pr-stack `orchestrate` prompt starts with a `<context-reminder>` header listing
      `exploration.md` when it exists, and no header when no docs exist.

## Testing Plan

**Test level**: Integration (workflow engine + mock backend) for artifact production and prompt
headers; Unit for schema validation, writer behavior, manifest registration, and prompt content.

Acceptance tests (integration, `packages/tddy-integration-tests/tests/exploration_artifact_acceptance.rs`):

1. `plan_submit_with_exploration_writes_exploration_md_under_artifacts` — run the plan flow with a
   mock backend submitting `exploration`; assert `artifacts/exploration.md` exists with the content.
2. `plan_submit_without_exploration_writes_no_exploration_md` — same flow without the field;
   assert the file is absent and PRD.md still written (guard).
3. `acceptance_tests_prompt_header_lists_exploration_md_when_present` — with `exploration.md`
   present, the acceptance-tests prompt's context header lists its absolute, existing path.
4. `red_prompt_header_lists_exploration_md_when_present` — same for the red prompt.
5. `green_prompt_starts_with_context_header_listing_exploration_md` — green (which lacks the
   header today) receives a context header listing `exploration.md`.

Unit tests: plan schema accepts/omits `exploration` (tddy-tools `schema.rs`); `write_artifacts`
writes/skips `exploration.md` (`writer.rs`); manifests register the artifact (tdd, tdd_small,
bugfix, grill-me); prompt templates contain the read/append instructions (`planning.rs`,
`acceptance_tests.rs`, `red.rs`, `green.rs`, grill-me prompt).

Strong assertions: exact file path under `artifacts/`, exact submitted content round-trip, header
line format `exploration.md: <absolute path>`, schema validation error absence/presence.

## Future Considerations (Not In Scope)

- Prime `exploration.md` from the FastContext discovery subagent's citations
  ([discovery-agent.md](discovery-agent.md), [managed-codebase-subagents.md](managed-codebase-subagents.md)).
- Structured exploration entries merged into `changeset.yaml` `discovery` (line/col-aware
  `relevant_code`).
- Staleness detection: flag exploration line references invalidated by later diffs.

## Related Documents

- [planning-step.md](planning-step.md) — planning phase, artifacts, discovery
- [workflow-recipes.md](workflow-recipes.md) — shipped recipes
- [workflow-json-schemas.md](workflow-json-schemas.md) — submit schemas
- [session-layout.md](session-layout.md) — `sessions/<id>/artifacts/` layout
- [discovery-agent.md](discovery-agent.md) — FastContext discovery subagent
