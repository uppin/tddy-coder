# 2026-11-07 — `tddy-session-create-web`'s Cypress support copies `tddy-web`'s

**Category:** Duplication (test support)
**Source:** `#web-split` 7/17 (PR #844), `/green`

The package's `cypress/support/` carries copies of support `tddy-web` still uses itself:
`servedAgents.ts`, `rpc/{responses,protoRpc,connectionRpcs,sessionFilesServiceBackend,sessionAgentRosterBackend,recordedRequests}`,
`util/fileDrop`, `pages/{createSessionPage,projectHomeSetupPage}`, and a trimmed `testIds.ts`. A change to
one copy (a served agent's metadata, a page-object selector, a recorded-request shape) silently misses
the other's specs.

**Why deferred:** `tddy-web`'s drawer specs (`CreateSessionAcceptance`, `…BranchConflictAcceptance`,
`ProjectHomeSetupPaneAcceptance`, …) still drive the form through these, and they move with
`feature/web-split/session`. No `#web-split` package exports Cypress support from its runtime entry yet,
so sharing them now means inventing that seam inside this node.

**What closing it takes:** when `session` moves the drawer specs, delete the `tddy-web` copies that have
no remaining user; for the rest, export them from a `testkit` entry (`tddy-session-create-web/testkit`
or `tddy-client-web/testkit`) and import them in both. Natural owner: the `#web-split` sweep node.
