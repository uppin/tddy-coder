# Changeset: Remove ghostty-web and rename the terminal components

**Date**: 2026-11-08
**Status**: 🚧 In Progress
**Type**: Refactoring
**Complexity**: 4 — rename across tddy-terminal-web, tddy-session-web, tddy-storybook, tddy-web and docs; dependency and CSP removal

**Stack**: `#xterm` 4/4 — branch `feature/xterm/drop-ghostty`, base `feature/xterm/xterm-engine`
**PR**: [#869](https://github.com/uppin/tddy-coder/pull/869)

## Initial Discovery

[2026-11-08-drop-ghostty-initial-discovery.md](./2026-11-08-drop-ghostty-initial-discovery.md)

## Affected Packages

- **tddy-terminal-web**: primary.
- **tddy-session-web**, **tddy-storybook**, **tddy-web**: consumers, specs and e2e as listed in Scope.

## Related Feature Documentation

- PRD: [PRD-2026-11-08-drop-ghostty.md](../../ft/web/1-WIP/PRD-2026-11-08-drop-ghostty.md)

## Summary

Remove `ghostty-web` and everything that existed only for it (`ghosttyEngine`, `lib/ghosttyInstance.ts`, the prebundle entry, Tauri's `wasm-unsafe-eval`, the storybook render exclusion), and rename `Ghostty*` to `Terminal*` across exports, testIds, story ids, Cypress commands, file names and docs.

## Background

After n3 no terminal uses ghostty-web. Its name remains on the public components and test hooks, which would mislead readers.

## Responsibility

Owns the removal list and the renamed public API (`TerminalView`, `TerminalSession` and their `Props`/`Handle` types).

## Boundaries

- No behaviour change.
- Rust test names are not renamed (todo).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n3 `feature/xterm/xterm-engine` | every terminal on `xtermEngine` | removes the now-unused ghostty path | re-open engine behaviour |

**Sequencing (developer, 2026-11-08, pass 2 review):**

- The storybook render test without the terminal-story exclusion fails today on ghostty-web's WASM fetch under happy-dom. **n3's green removes that cause** (components construct `xtermEngine`); **n4's green confirms the result** and fixes any xterm/happy-dom issue the story then hits.
- **n4's green rewrites n2's `src/engine/engineBoundary.test.ts`**, which names `ghosttyEngine` / `ghostty-web` and is listed by `scripts/no-ghostty.test.ts`.
- `scripts/no-ghostty.test.ts` runs in CI through `.github/workflows/web.yml` "Web rule tests" (wiring approved by the developer).

## Draft PR contract

First push (commit 2), not this PR's deliverable: A rule test (`scripts/no-ghostty.test.ts`) asserting no `ghostty` in web package sources, specs, testIds or `package.json`s, and the storybook render test without the exclusion — failing until the removal and rename land.

## Green wave

**Wave:** 4 of 4
**Greenable independently:** after n3
**Concurrent with:** none — the stack is serial
**Blocks:** none

Real dependency edges: `n1 → n2 → n3 → n4`. Pass 2 waits until `#web-split` is green (developer, 2026-11-08).

## Scope

- [x] Remove ghostty-web + ghosttyEngine + instance + prebundle entry + CSP entry + storybook exclusion
- [x] Rename exports, types, testIds, story ids, Cypress command, files, docs
- [x] Rule test green
- [x] Todo for Rust e2e test names

## Technical Changes

### State A (Current)

ghostty-web is installed but unused; public names say Ghostty.

### State B (Target)

ghostty-web is gone; public names say Terminal.

## Testing Plan

Cypress component specs one at a time locally, `bun test src`, scoped to the touched packages. CI: `web.yml` affected-package shards.

## Acceptance Tests

- `scripts/no-ghostty.test.ts` — no ghostty in web packages.
- `tddy-storybook` `storiesRender.test.tsx` — renders the terminal story.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- Developer, 2026-11-08: WebGL renderer with DOM fallback; deps `@xterm/xterm`, `@xterm/addon-fit`, `@xterm/addon-unicode11`, `@xterm/addon-webgl`, `@xterm/addon-clipboard`; rename inside the last node; stack registered as an extension of `#web-split`.

## Refactoring Needed

## Validation Results

## TODO

- [x] Record initial discovery
- [x] Cross-check code issues and TODOs
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run the touched packages' tests — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Final validation (/validate-changes)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
