# 2026-08-01 — tddy-web — inactive session activities follow-ups

**Category:** Future enhancement
**Source:** inactive-session-activities changeset, 2026-08-01

- **The component harness renders without any CSS, so no Cypress component test can assert layout.**
  `cypress/support/component-index.html` loads no stylesheet and `cypress/support/component.ts`
  imports none, so every Tailwind class is inert and every element measures the full viewport width.
  This was discovered attempting to pin "the inspector is a ~360px overlay, not the full pane" after
  `data-docked` was removed — the assertion failed with `expected 1280 to be below 1280`. **Do not
  re-attempt geometry assertions in component specs** without first importing the app stylesheet into
  the harness; until then, the *removal* of inspector docking has no direct test pinning it (the
  specs prove the adjacent fact that the base view stays mounted behind an open drawer). Importing
  the stylesheet would make layout testable but risks perturbing the ~163 existing specs, so it is
  its own changeset.
- **Resume has up to a ~2s dead time before the view changes.** The base view is derived from
  session liveness, and liveness for the selected session only refreshes on the drawer's 2s
  `ListSessions` poll (`sessionManager.ts` `REFRESH_INTERVAL_MS`), so after clicking Resume the pane
  keeps showing the recorded transcript until the next poll reports the session live. Not a
  regression — it falls straight out of "the view is derived, not navigated" — but it is a visible
  lag on this feature's primary action. An optimistic local liveness hint on a successful
  `ResumeSession` would close it without reintroducing view state.
