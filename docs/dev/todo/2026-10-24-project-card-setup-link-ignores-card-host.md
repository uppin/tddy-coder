# 2026-10-24 — A Projects card's "Set up main" link reflects one host; the setup pane acts on another

**Category:** Bug risk (low) — design choice for the developer
**Source:** 2026-10-24-project-home-from-new-session-form

- `ProjectMainSessionLink` (`packages/tddy-web/src/components/projects/ProjectMainSessionLink.tsx`)
  reads **Set up main** or **Open main** from the card's **primary host** (`group.hosts[0]`'s
  `homeSessionId`, passed by `ProjectCard.tsx`). Main is recorded per host.
- The link's target, `#/sessions/project-home/<id>`, names only the project. The setup pane
  (`ProjectHomeSetupPane`) and `EnsureProjectHomeSession` act on the sessions drawer's **selected
  host**. When that is not the card's primary host, the card can say *Set up main* while the drawer's
  host already has a main (the pane then replaces it), or *Open main* for a session the selected
  host does not list.
- **Why deferred:** found by `/validate-changes` after green; which host the link should mean is a
  product decision (the card's primary host, a per-host link on each host row, or the drawer's
  selection) and was left to the developer rather than chosen here.
- **What would close it** (follow-up PR: put the host on the setup route): give the setup route the
  host it acts on (for example `?host=<daemonInstanceId>` on `#/sessions/project-home/<id>`, or a
  link per host row), have the card link carry the host whose row it read, and pin it with a case in
  `packages/tddy-web/cypress/component/ProjectHomeSetupLinkAcceptance.cy.tsx` where the card's
  primary host and the drawer's selected host differ.
