# Planning Phase — Shared Reference

Canonical steps for interviewing the user, analyzing the codebase, and producing PRD + changeset documents. Referenced by planning skills and commands — follow these steps exactly.

## Step 1: Interview the User

**MANDATORY** — Ask the user these questions before proceeding. Do NOT skip or assume answers.

1. **What** — What is the feature or change? New feature or modification to existing?
2. **Why** — What problem does this solve? What is the motivation?
3. **Where** — Which packages / product areas are affected?
4. **Constraints** — Any specific requirements, performance targets, compatibility needs?
5. **UX** — Design preferences, user experience expectations?
6. **Scope** — What is explicitly out of scope?

Adapt follow-up questions based on answers. Keep interviewing until requirements are clear enough to write the PRD.

## Step 2: Analyze Existing Code

**MANDATORY** — Read the codebase before writing any documents. Persist the full exploration
in `docs/dev/1-WIP/{changeset-slug}-initial-discovery.md` before creating the PRD or changeset.
Follow `.agents/skills/planning/references/initial-discovery.md` exactly.

Pick `{changeset-slug}` (`YYYY-MM-DD-feature-name`) now — it is the upcoming changeset basename.

**How to explore**:
- **On Cursor**: launch the `explore` subagent via `Task` (`subagent_type: "explore"`) with the
  dump contract from `initial-discovery.md`.
- **On other harnesses**: use the equivalent Explore agent, or run Grep / Glob / Read yourself.

Record **every** pass: inspected files and code excerpts, grep/glob patterns, and the sequence of
the exploration. Combined conclusions go at the **top** of the discovery file; each pass is a
separate `## Exploration N` section at the **tail**. Further explorations during later steps
append a new pass and rewrite Combined Conclusions — they do not replace earlier passes.

For each affected package:
1. Read `packages/{package}/README.md`
2. Read `packages/{package}/docs/` (if exists)
3. Read relevant source files to understand current architecture, APIs, and behavior
4. Check `docs/dev/1-WIP/` for conflicting active changesets
5. Check `docs/ft/{product-area}/` for existing feature docs

Repo-wide conventions (toolchain, scripts, judgment boundaries) live in `CLAUDE.md` at the repo
root — read it when the plan touches build, test, or install workflows.

Document findings as **State A** (current state) — this becomes the baseline for the changeset.
State A in the changeset is distilled from the discovery file; do not dump grep traces into the
changeset.

## Step 3: Identify Product Area

Determine which product area the feature belongs to (the subdirectories of `docs/ft/`):
- `build` — Build backends and image/artifact pipelines
- `coder` — The coding agent, workflow, and TUI
- `daemon` — Session daemon, RPC surface, orchestration
- `desktop` — Desktop application
- `screen-capture` — Screen capture and streaming
- `supervisor` — Privileged supervisor and brokered child processes
- `vm` — Virtual machines and sandbox guests
- `web` — Web dashboard
- Other — create new product area directory if needed

## Step 4: Create PRD Document

**Location**: `docs/ft/{product-area}/1-WIP/PRD-YYYY-MM-DD-feature-name.md`

Follow the PRD template from `.cursor/rules/prd-doc.mdc`.

Include:
- Header metadata (date, PRD type)
- Affected features (ALL feature documents impacted)
- Summary and background
- Proposed changes (what's changing, what's staying the same)
- Impact analysis (technical + user)
- Implementation plan
- Acceptance criteria with checkboxes
- References to affected features

Update `docs/ft/{product-area}/1-OVERVIEW.md` with a reference to the new PRD.

**Present PRD to user and wait for approval before proceeding.**

## Step 5: Create Changeset Document

**Location**: `docs/dev/1-WIP/YYYY-MM-DD-feature-name.md`

Follow the changeset template from `.cursor/rules/changeset-doc.mdc`.

Include all required sections:
- Header metadata (date, status: `🚧 In Progress`, type)
- **Initial Discovery** — first content section after the header; link to
  `./{changeset-slug}-initial-discovery.md` (see `initial-discovery.md`). The discovery file
  must already exist from Step 2.
- Affected packages (ALL packages with links to READMEs and docs)
- Related feature documentation (link to PRD from Step 4)
- Summary and background
- Scope (high-level deliverables with checkboxes)
- Technical changes:
  - **State A** (from Step 2 analysis)
  - **State B** (target implementation)
  - **Delta** (per-package changes)
- Implementation milestones (specific, measurable checkboxes)
- Testing plan:
  - Determine appropriate test level (E2E / Integration / Unit)
  - Testing options analysis with trade-offs
  - Coverage requirements
- Acceptance tests (per-package, with descriptive names and file paths)
- Technical debt & production readiness (empty, populated during development)
- Decisions & trade-offs (document choices made during planning)
- Refactoring needed (empty sections per validation phase)
- Validation results (empty sections per validation command)

**CRITICAL**: Take extra time on the testing strategy. This directly feeds the red phase — every acceptance test defined here becomes a real test file. Define:
- Descriptive test names that read as behavior specifications
- Exact file paths where each test will live (e.g. `packages/{package}/tests/{name}.rs` for
  integration tests, `packages/{package}/src/{module}.rs` for unit tests in a `#[cfg(test)]` module)
- What each test validates and why it matters

**MANDATORY**: Include a `## TODO` checklist in the changeset document. The calling skill/command specifies which items to check off — at minimum the first two (PRD + changeset) are checked by this phase.

```markdown
## TODO

- [x] Record initial discovery (`YYYY-MM-DD-feature-name-initial-discovery.md`)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run all tests (`./test`) — verify 100% pass
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
- [ ] Linting and formatting (`cargo clippy -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `{slug}-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
```

Scope test runs to the packages you touched while iterating (`./test -p {package}`), and run the
full `./test` before claiming the checklist item above is done. `cargo build` covers compilation
when you only need to check that the workspace still builds.

## Rules

- Never skip the user interview — requirements come from the user, not assumptions
- Never skip code analysis — State A must reflect actual current implementation
- Never skip the initial-discovery companion — write
  `docs/dev/1-WIP/{changeset-slug}-initial-discovery.md` during Step 2 (full Explore dump, not a
  summary). Append later passes; wrap deletes the file.
- Always read existing docs/code before writing State A
- Always present PRD for user approval before creating changeset
- Take extra time on testing strategy — don't rush
- Determine appropriate test level based on changeset scope
- List ALL affected packages, even for minor changes
- Make milestones specific and measurable
- Define acceptance tests with descriptive names and file paths
- Don't modify `packages/*/README.md` or `packages/*/docs/` directly — use the changeset workflow
- Don't create changeset without reading State A docs
- Don't include process instructions in changeset content
- Quality first — never compromise document quality
