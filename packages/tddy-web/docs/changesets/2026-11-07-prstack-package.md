# 2026-11-07 — PR-stack screens moved to `tddy-prstack-web` (#web-split 14/17, PR #850)

`workflowViews` renders `PrStackScreen` and `WorkflowChatScreen` from the package and wraps them in the
start-passphrase scope the package no longer mounts itself. The drawer-driven PR-stack specs
(`PrStack*`, `PlannedPr*`, `WorkflowChatViewRouting`, `PrStackStartSessionDocAttachments`, …) stay
here, importing from the package; their page objects and fixtures are copied into the package for its
own specs. `aStackNode`, `aStackChildSession` and `aBranchResolution` left `src/test-utils/builders.ts`.
