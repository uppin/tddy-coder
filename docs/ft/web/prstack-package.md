# PR-stack package (`tddy-prstack-web`)

**Package:** `packages/tddy-prstack-web`
**Applies to:** the PR-Stack screen and its chat, and the workflow chat screen in the session drawer
**Added:** 2026-11-07 (#web-split 14/17, PR #850)

## Overview

The PR-stack orchestrator's screens are one package: the planned-PR list and panel with live branch,
PR and base-sync status, reorder and repoint, starting a planned PR's session, the orchestrator chat and
the workflow chat screen. It reaches daemons only through `tddy-client-web`, renders with `tddy-ui-web`,
opens sessions through `tddy-session-create-web` and chat through `tddy-chat-web`; it does not depend on
the web app or on LiveKit.

## Behaviour

- PR-stack behaviour is as before ([pr-stacking.md](../coder/pr-stacking.md),
  [pr-stack-live-status.md](../coder/pr-stack-live-status.md),
  [session-drawer.md § PR-Stack Chat Screen](session-drawer.md)).

## Acceptance criteria

- [x] PR-stack lives in `tddy-prstack-web` and imports nothing from `tddy-web` or LiveKit (rule test).
- [x] Its specs pass from the package (`web.yml` runs them; `Web lint` typechecks it).
- [x] The drawer renders PR-stack sessions as before (`tddy-web`'s drawer-driven PR-stack specs pass).

Technical reference: [packages/tddy-prstack-web/docs/README.md](../../../packages/tddy-prstack-web/docs/README.md).
