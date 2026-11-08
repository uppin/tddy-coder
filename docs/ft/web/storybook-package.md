# Storybook package (`tddy-storybook`)

**Package:** `packages/tddy-storybook`
**Applies to:** every web component package's stories, and the static Storybook the e2e specs run against
**Added:** 2026-11-07 (#web-split 19/20, PR #852)

## Overview

There is one Storybook for the web packages, and it owns every story; component packages carry no
Storybook dependency. Stories are grouped by package and render against the same port doubles the
packages' component specs use, so a story needs no daemon.

## Behaviour

- `bun run storybook` (root) serves it; the moved stories keep their ids, so the e2e specs that visit
  them are unchanged.
- Every component package has at least one story for its main screen or component.

## Acceptance criteria

- [x] No component package contains a story (rule test).
- [x] Every story renders without an error (render test; the terminal story in the static build).
- [x] `tddy-web`'s Storybook e2e specs pass against the package's build (`storybook.cy.ts`,
  `ghostty-terminal-stories.cy.ts`).

Technical reference: [packages/tddy-storybook/docs/README.md](../../../packages/tddy-storybook/docs/README.md).
