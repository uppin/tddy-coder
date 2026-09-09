# 2026-09-06 — The route-dispatch joins in `index.tsx` are uncovered

**Category:** Future enhancement
**Source:** host registry, #453 (`#hosts-screen` 1/8)

`packages/tddy-web/src/index.tsx` dispatches each route as one line —
`isHostsPath(path) ? <HostsAppPage/> : …`. Both halves either side of it are tested: the menu→path
half by a nav acceptance spec, the path→predicate half by `appRoutes.test.ts`. The line that joins
them is not, so a branch wired to the wrong component would ship green.

Covering it needs the whole app mounted rather than a component test, and **every** route in that
chain has the identical gap — so the fix is one harness, not one spec per screen. Recorded when
`#/hosts` was added rather than fixed there, because a route-dispatch harness is not that node's
work.

Feature: [`hosts-screen.md`](../../ft/web/hosts-screen.md).
