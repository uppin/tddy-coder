---
name: planning
description: Plan product and technical work before implementation by creating or updating PRDs, changesets, and formal planning docs. Backs `/plan-red`, `/plan-ft` and `/plan-ft-dev`. Use when the user wants feature planning, a requirements change documented, or planning material turned into formal context docs. Use `tdd` instead when the request is specifically for a one-shot TDD delivery plan.
---

# Planning

Create the planning artifacts before coding begins.

## Workflow

1. Clarify the request.
Ask only the minimum concrete questions needed to understand scope, affected areas, constraints, and whether this is new work or a change.

2. Create or update product documentation first.
Use `docs/ft/` and the current repo conventions for PRDs, features, and requirement changes.

3. Ask the user to switch to Plan mode if they want collaborative planning there.
Do not assume you can switch modes yourself. If they stay in the current mode, continue planning in-thread.

4. Create the technical delta in `docs/dev/1-WIP/`.
Keep implementation tracking in the changeset workflow. During code analysis, write
`docs/dev/1-WIP/{changeset-slug}-initial-discovery.md` with the full Explore / Discovery dump
(see `references/initial-discovery.md`). The changeset's first content section after the header
is `## Initial Discovery`, linking to that companion. Do not update `packages/*/README.md` or
`packages/*/docs/` during planning.

5. Produce a detailed execution checklist.
Break the work into discrete TODOs covering acceptance tests, TDD execution, validation, documentation wrap-up, and final review.

## Use These Source Prompts

- `.agents/commands/plan-ft.md`
- `.agents/commands/plan-ft-dev.md`
- `.agents/commands/plan-red.md`
- `.agents/commands/requirements-change.md`

## References

Read these only when needed:

- `references/planning-phase.md`
- `references/initial-discovery.md`
- `.cursor/rules/prd-doc.mdc`
- `.cursor/rules/changeset-doc.mdc`
- `.cursor/rules/feature-doc.mdc`
- `.cursor/rules/dev-doc.mdc`

## Guardrails

- Prefer a single coherent plan over scattered notes.
- Keep product intent and technical delta separate.
- Use `docs/dev/1-WIP/` for in-progress technical docs.
- Persist every Explore / Discovery pass in `{changeset-slug}-initial-discovery.md`; wrap deletes it.
- Leave stable package docs for the wrap/finalization step.
