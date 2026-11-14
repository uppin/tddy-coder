# 2026-11-14 — A project's stacks open by id from the drawer

The session drawer's project groups list the project's PR stacks in a Stacks row, across hosts.
Selecting one opens the full-width stack view at `#/sessions/project/<projectId>/stack/<stackId>`,
with the Planned PRs panel's controls acting on that stack. A pr-stack orchestrator's chat screen
loads its panel by its stack id. Hosts that did not answer a listing are named in the row.
