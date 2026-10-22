# 2026-10-22 — The tool-session provider picker needs harness metadata from the daemon

**Category:** Deferred from `#provider-route` 4/9 (#665)
**Source:** `/pr-wrap` on `feature/provider-route/tool-route` (two `TODO(tool-route)` markers)

The create-session provider field is offered for tool sessions now, keyed by the selected agent. Two
things it shows are not right for every agent:

1. **The built-in option is always labelled "Anthropic (built in)"**
   (`packages/tddy-web/src/components/sessions/CreateSessionProviderField.tsx`). For a `codex` or
   `codex-acp` tool session the vendor is OpenAI.
2. **A specialized agent definition shows the daemon's "unknown harness" reason**
   (`packages/tddy-web/src/components/sessions/CreateSessionPane.tsx`). The agent catalog does not say
   which agents are definitions rather than harnesses, so the form asks `ListProviderRoutes` for one
   anyway.

Neither is dangerous: the start is refused for a definition with an account, and the built-in option
sends the same request whatever its label.

**Why deferred:** both need the daemon to say more — a vendor name on `ListProviderRoutesResponse` and a
harness/definition flag on the catalog's agents — which is a change to `accounts.proto` and the
`RouteSupport` port. `#provider-route` 5/9 edits that same message and service, so adding it here
would conflict on the cascade, and keeping a vendor table in the web would restate the daemon's matrix.

**What would close it:** `ListProviderRoutesResponse.harness_vendor` (from `Harness`), the field's
built-in option labelled from it, and the pane not asking for routes for an agent the catalog marks as
a definition.
