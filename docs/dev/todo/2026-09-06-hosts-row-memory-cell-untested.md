# 2026-09-06 — The Hosts row memory cell ships untested

**Category:** Missing coverage
**Source:** `#hosts-screen` 3/8, PR #455

- `HostRowTelemetry` renders `hosts-row-<id>-memory`, and the node's `## Responsibility` called for
  it, but no test asserts it. Footer coverage exists
  (`HostResourcesAcceptance.cy.tsx`); the row cell has none.
- **It is writable now.** `#hosts-screen` 2/8 established `mountCells`, which mounts
  `HostRowTelemetry` directly and so does not depend on the Hosts screen rendering rows. The work is
  a `memory:` accessor on `hostTelemetryPage` (`cypress/support/pages/hostsScreenPage.ts`) plus one
  test in the same shape as the existing `cpu` / `disk` ones.
- The node's plan named row tests in `HostsScreenTelemetryAcceptance.cy.tsx`; the red phase
  consolidated them into `HostResourcesAcceptance.cy.tsx` as footer-only coverage, and the row case
  was not carried across. Deferred at wrap time by operator decision, recorded here rather than lost.
