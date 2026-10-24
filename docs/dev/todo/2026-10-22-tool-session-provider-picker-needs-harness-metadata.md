# 2026-10-22 — The tool-session provider picker needs harness metadata from the daemon

**Category:** Deferred from `#provider-route` 4/9 (#665)
**Source:** `/pr-wrap` on `feature/provider-route/tool-route` (two `TODO(tool-route)` markers)

The create-session provider field is offered for tool sessions now, keyed by the selected agent.

~~**The built-in option is always labelled "Anthropic (built in)"**~~ — closed: the option is labelled
**Default**, which is true for every harness, so it needs no vendor name from the daemon.

What is left: **the pane asks `ListProviderRoutes` for a specialized agent definition**
(`packages/tddy-web/src/components/sessions/useCreateSessionModel.ts`). The agent catalog does not say
which agents are definitions rather than harnesses, so the form asks anyway. The daemon answers
"unknown harness", which now hides the field instead of showing that reason, so nothing wrong is
visible — but it is a request that should not be made.

**What would close it:** a harness/definition flag on the catalog's agents, and the pane not asking
for routes for an agent the catalog marks as a definition.
