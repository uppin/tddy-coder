# Changeset: LiveKit RPC transport package rename (`tddy-livekit-rpc-web`)

**Date**: 2026-11-07
**Status**: 🚧 Green done (rename applied; Cypress left to CI)
**Type**: Architecture Change
**Complexity**: 4 — mechanical, but more than one package (the renamed package, `tddy-web`, `tddy-desktop` consumers, CI workflow, scripts)

**Stack**: `#web-split` 4/16 — branch `feature/web-split/livekit-rpc`, base `feature/web-split/ui` (#840)

**PR**: [#841](https://github.com/uppin/tddy-coder/pull/841)

## Initial Discovery

[2026-11-07-livekit-rpc-package-rename-initial-discovery.md](./2026-11-07-livekit-rpc-package-rename-initial-discovery.md)

## Prerequisites

### ⚠ DURING — `Web tests` run LiveKit from the floating `:master` image — [`2026-10-18-web-tests-start-livekit-from-the-floating-master-image.md`](../todo/2026-10-18-web-tests-start-livekit-from-the-floating-master-image.md)

`cypress/support/livekitDockerTestkit.ts` and the image pull step (now in the package's workflow)
move with the rename. Recorded, not fixed here — the entry wants its own change; the entry's file
paths are updated to the new package name.

### ⚠ DURING — `startEchoServer` intermittently exceeds its task timeout — [`2026-10-18-livekit-web-start-echo-server-intermittently-exceeds-its-task-timeout.md`](../todo/2026-10-18-livekit-web-start-echo-server-intermittently-exceeds-its-task-timeout.md)

The `FIXME(livekit-web-echo-server-hang)` debug env moves into the renamed workflow; the debug
namespace `tddy:livekit-web:*` is **kept** as is, so existing logs and the entry stay accurate.

## Affected Packages

- **tddy-livekit-rpc-web** (renamed from `tddy-livekit-web`): README, `docs/`, `package.json`,
  `cypress.config.ts` (JUnit name), `scripts` (`generate` path).
- **tddy-web**: `package.json` (dependency + `prebuild`), `vite.config.ts` and `tsconfig.json` aliases,
  the 10 source files importing it, `docs/`.
- **tddy-desktop**: any reference to the package name.
- **root** `package.json` (`workspaces`), `bun.lock`, `scripts/` naming the package path,
  `.github/workflows/web-tddy-livekit-web.yml` → `web-tddy-livekit-rpc-web.yml`.
- Living docs naming it: `docs/dev/guides/ci.md`, `.agents/commands/*`, `docs/ft/web/*.md` (at wrap),
  open `docs/dev/todo/` entries. Historical changesets and changelog entries are **not** rewritten.

## Related Feature Documentation

- PRD: [PRD-2026-11-07-livekit-rpc-package-rename.md](../../ft/web/1-WIP/PRD-2026-11-07-livekit-rpc-package-rename.md)

## Summary

Rename the LiveKit RPC transport package so `tddy-livekit-web` can become the LiveKit wiring package.

## Background

See the PRD.

## Responsibility

- The package's new name and path, `packages/tddy-livekit-rpc-web`, with unchanged exports.
- Every live reference to the old name as the transport.
- The rule test that no workspace manifest, alias, workflow or script names `tddy-livekit-web` as the
  transport (the name is reserved for `feature/web-split/livekit`).
- The renamed workflow.

## Boundaries

- No transport code or test changes beyond names; no new package named `tddy-livekit-web` (that is
  `feature/web-split/livekit`).
- Historical records (`docs/dev/changesets/`, `docs/ft/*/changelog/`, `packages/*/docs/changesets/`)
  stay as written.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` web-ci (#838) | `web-package.yml`, `web-tddy-livekit-web.yml` caller with `needs-rust-fixture` / `needs-livekit-image` | renames that caller | change the shared workflow's inputs |
| `n2`, `n3` (#839, #840) | — (branch predecessors only) | nothing | — |

## Draft PR contract

First push (commit 2): the rule test `scripts/livekit-rpc-package-name.test.ts`, failing while the
transport sits at the old path and name. The workflow rename happens at green, once #838's
`web-tddy-livekit-web.yml` exists on the base.

## Green wave

**Wave:** 2 of 9
**Greenable independently:** yes — a rename with a name rule; no behaviour from n2/n3.
**Concurrent with:** `react-commons` (#839), `ui` (#840)
**Blocks:** `feature/web-split/livekit` (needs the name free and the transport at its new path).

    n1 → n4      n4 → n6

## Scope

- [x] `git mv packages/tddy-livekit-web packages/tddy-livekit-rpc-web`; manifest + exports + scripts
- [x] `tddy-web`, `tddy-desktop`, root workspaces, aliases, imports updated
- [x] Workflow renamed; rule test green
- [ ] Living docs updated (at wrap)

## Technical Changes

### State A (Current)

`packages/tddy-livekit-web`: LiveKit ConnectRPC transport (`transport.ts`, `transport-factory`,
`chunking.ts`, `async-queue.ts`, generated code), Cypress specs with a Docker LiveKit + `echo_server`
example. Referenced by 65 files outside `bun.lock` — most are historical docs; the live ones are
`package.json` (root workspaces), `packages/tddy-web/{package.json, vite.config.ts, tsconfig.json}`,
10 `tddy-web` source files, `scripts/`, the CI workflow, guides and agent commands.

### State B (Target)

Same package at `packages/tddy-livekit-rpc-web`, name `tddy-livekit-rpc-web`. No live reference to
`tddy-livekit-web`.

### Delta

- Directory rename; manifest name; consumers; workflow; JUnit file name `livekit-rpc-web-component-*.xml`.

## Implementation Milestones

- [x] Rule test green
- [ ] `tddy-livekit-rpc-web` Cypress specs green from its renamed config — NOT RUN in green (needs a Docker LiveKit and an `echo_server` build in a cold target/); left to CI; package unit tests (44) and `tsc` build pass
- [x] `tddy-web` builds (`bun run build`)

## Testing Plan

**Level:** a `bun test` name rule + the package's existing Cypress specs, unchanged, as the regression.

## Acceptance Tests

### scripts (`./dev bun test ./scripts/livekit-rpc-package-name.test.ts`)

- `the transport package is named tddy-livekit-rpc-web and has its own web workflow`
- `no file reaches a transport export through tddy-livekit-web` — transport exports read from the
  package entry point; every `import { … } from "tddy-livekit-web"` in `packages/*/{src,cypress}` checked
- `whatever package is named tddy-livekit-web is not the transport` — no `transport.ts`,
  `transport-factory.ts`, `chunking.ts` left at the old path
- `the codegen manifest generates the transport's code at its new path`

These pin the transport's identity rather than banning the old name, because `feature/web-split/livekit`
gives `tddy-livekit-web` to the wiring package. Contract run: 4 failing (no package at the new path;
10 `tddy-web` files import transport exports through the old name; transport sources at the old path;
manifest rows at the old path).

### tddy-livekit-rpc-web

- Existing `cypress/component/*` specs pass from the renamed package (baseline, unchanged).

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **Debug namespace kept** (`tddy:livekit-web:*`): renaming log namespaces is churn with no reader benefit.

## Refactoring Needed

### From @validate-changes (Change Validation)

### From @validate-tests (Test Quality)

### From @prod-ready (Production Readiness)

### From @analyze-clean-code (Code Quality)

## Validation Results

## TODO

- [x] Record initial discovery (`2026-11-07-livekit-rpc-package-rename-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer waived the per-node review for this stack, 2026-11-07: "finish the rest without stopping")
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (rule test; `tddy-livekit-rpc-web` Cypress) — verify 100% pass
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
- [ ] Linting and formatting
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
