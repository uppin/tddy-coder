# PRD: Grill-me workflow recipe

**Date**: 2026-04-03  
**Status**: In progress (two-goal model; see `workflow-recipes.md`, Updated: 2026-04-04)  
**Affected features**: [workflow-recipes](../workflow-recipes.md)

## Summary (Updated: 2026-04-05)

Ship a **`grill-me`** workflow recipe with **two goals**: **`grill`** then **`create-plan`**. The **Grill** phase issues clarification through **`InvokeResponse.questions`** (presenter + **tddy-tools** path). The **Create plan** phase consumes Q&A and the original feature input, then writes **`artifacts/grill-me-brief.md`** (problem, Q&A, analysis, preliminary implementation plan) under the session layout. For **version control in the target repo**, persist that plan output under a path documented in **[AGENTS.md](../../../../AGENTS.md)** (**Documentation Hierarchy** → **`plans/`**): if a feature document specifies another repo path, use that; otherwise default to **`plans/<SOME-PLAN-NAME>.md`** at the repo root. Selection is via `--recipe grill-me`, `changeset.yaml`, daemon `StartSession`, and the web **Start New Session** recipe control.

**Implementation**: See [2026-04-03-grill-me-recipe-changeset.md](../../../dev/1-WIP/2026-04-03-grill-me-recipe-changeset.md).

## Background

Today, **`free-prompting`** offers an unconstrained loop; **`tdd`** and **`bugfix`** target implementation workflows. Teams need a structured **discovery / elicitation** path that produces a reviewable brief before coding. The engine already supports clarification (`InvokeResponse.questions`) and answer forwarding (`answers` → `prompt` in hooks).

## Proposed changes (Updated: 2026-04-04)

- **`GrillMeRecipe`**: **two goals** — **`grill`** (clarify only) and **`create-plan`** (write brief). Graph: **`grill` → `create-plan` → `end`**. CLI recipe name stays **`grill-me`**.
- **Grill**: System prompt for **questions only**; uses **AskQuestion / clarification** tools. **`GrillMeWorkflowHooks`** preserve answers→prompt; stream agent output like free-prompting. When the backend returns **no `questions`**, the invoke task **`Continue`s** to **Create plan** (same no-submit semantics as **`free-prompting`** / **`prompting`**).
- **Create plan**: System prompt requires **`artifacts/grill-me-brief.md`** and required sections; hooks pass **Q&A + user input** into the user message (from **`feature_input`**, **`output`**, **`answers`**).
- **Repo persistence (Updated: 2026-04-05)**: The **Create plan** brief is committed to the **working tree** per **[AGENTS.md](../../../../AGENTS.md)**: prefer a path named in **`docs/ft/`** for the feature; else **`plans/<SOME-PLAN-NAME>.md`** at repo root (see **Documentation Hierarchy**).
- **Session artifacts**: **`grill_brief` → `grill-me-brief.md`**; **`goal_requires_session_dir`**: **true** for **`grill`** and **`create-plan`**.
- **Resolver / policy**: unchanged CLI name **`grill-me`**; session-document approval skipped for v1 (`uses_primary_session_document`: **false**).
- **Product**: CLI help, **tddy-web** recipe dropdown, `docs/ft/coder/workflow-recipes.md`.

## Impact

### Technical

- New module alongside `free_prompting`; no `goals.json` / schema pipeline changes.

### User

- Clear recipe choice for “interview me, then write a brief” sessions.

## Success criteria

1. `grill-me` resolves from CLI, YAML, and daemon; unknown recipes list all supported names including **`grill-me`**.
2. Recipe metadata: `start_goal` **`grill`**, initial state **`Grill`**, **`goal_ids`** include **`grill`** and **`create-plan`**, **`uses_primary_session_document`** **false** (v1).
3. System prompts: **Grill** emphasizes structured questions; **Create plan** requires brief path/sections and consumes prior Q&A (covered by unit tests).
4. Full test suite passes; docs updated.
5. **Persistence**: Operators (or follow-up automation) place the **Create plan** brief into the repo per **AGENTS.md** (`plans/` default or feature-doc path).
