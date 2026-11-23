# 2026-10-25 — The New-session form offers 8 of the daemon's 10 workflow recipes

**Category:** Deferred from `#start-parity` 2/9 (#751)
**Source:** the `start-tool` changeset (#751) § Boundaries — "Does not change the form's behaviour"

The form's recipe picker reads a hardcoded `WORKFLOW_RECIPES` list
(`packages/tddy-web/src/components/sessions/createSessionRecipes.ts`, 8 entries), while the daemon
supports 10 (`tddy_workflow_recipes::approval_policy::supported_workflow_recipe_cli_names()`), which
is what `StartSession`'s schema offers an orchestrator. A person cannot pick the two the form omits.

**Why deferred:** #751 makes the tool match the form, not the form match the daemon; changing the
form was out of its scope.

**What would close it:** have the form read the daemon's recipe list (a per-host discovery RPC, as
`#start-parity` 5/9 `start-options` adds for the tool) instead of a constant, and pin it in a Cypress
spec.

**Re-measured 2026-11-22 (`#workqueue` 1/14, #1002):** the form's `WORKFLOW_RECIPES`
(`packages/tddy-session-create-web/src/createSessionRecipes.ts`) and
`supported_workflow_recipe_cli_names()` now name the **same 7 recipes**, so no recipe is unpickable
today. The remainder is the drift hazard: the form still lists a constant, so the next recipe the
daemon gains is missing from the form until someone edits it. The close criterion above is unchanged.
