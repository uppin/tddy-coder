# 2026-10-23 — Clicking "main" with no home agent opens that project's home-agent picker

**Type:** Fix

A project's pinned **main** entry in the sessions drawer was `aria-disabled` when the project had no
home agent, and its click returned early. The only explanation was a small grey hint, so clicking
"main" produced nothing: no request, no navigation, no error. This was reported from a live
`./web-dev` where every project lacked a home agent, and confirmed in a headless browser: no
`EnsureProjectHomeSession` was sent and the daemon never received a `SetProjectHomeAgent`.

The click now takes the operator to **Projects** at `#/projects?project=<id>`, on the same host,
with that project's **Home agent** picker focused and scrolled into view. A project that has a home
agent still starts its main session. No daemon or RPC changes. Stack `#visible-failures` 2/2
([#707](https://github.com/uppin/tddy-coder/pull/707)), on
[#706](https://github.com/uppin/tddy-coder/pull/706).

Permanent docs:
- [session-drawer.md § Pinned main entry](../../ft/web/session-drawer.md#pinned-main-entry)
- [projects-screen-multi-host.md § Home agent](../../ft/web/projects-screen-multi-host.md#home-agent)
- [url-state-routing.md § Params](../../ft/web/url-state-routing.md#params) (`project` on `#/projects`)
- [projects-screen.md § Home agent](../../../packages/tddy-web/docs/projects-screen.md#home-agent)

Package entry:
- [tddy-web](../../../packages/tddy-web/docs/changesets/2026-10-23-project-home-setup-link.md)

Deferred to the backlog, developer-approved:
- `2026-10-23-sessions-drawer-screen-at-500-lines`: `SessionsDrawerScreen.tsx` went 498 → 500 lines
  (two lines of prop wiring); the split waits for the stack to land, because #706 also touches the
  file. Tracked as the code issue `oversized-file-sessions-drawer-screen`.

Verification was local and scoped; full Cypress is CI's:
- `bun test src/routing/projectSetupLocation.test.ts`: 3/3;
- Cypress component `ProjectHomeSetupLinkAcceptance` 4/4, `SessionsDrawerProjectHomeAcceptance` 7/7,
  `ProjectsScreenHomeAgentAcceptance` 2/2;
- `tsc --noEmit` reports no error in a touched file.
