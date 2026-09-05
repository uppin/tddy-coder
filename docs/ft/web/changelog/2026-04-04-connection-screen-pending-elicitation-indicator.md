# 2026-04-04 — Connection screen: pending elicitation indicator

- **`SessionEntry`**: **`pending_elicitation`** on **`ListSessions`** (proto field **14**); generated clients expose **`pendingElicitation`**.
- **`ConnectionScreen`**: Session rows show an **Input needed** badge when **`pendingElicitation`** is true; each row sets **`data-pending-elicitation`** on the **`<tr>`**; badge **`aria-label`** for screen readers. Cypress **`ConnectionScreen.cy.tsx`** covers true/false cases.
- **Feature doc**: [web-terminal.md](../web-terminal.md) (Pending elicitation on session rows). Cross-package note: **[docs/dev/changesets/](../../../dev/changesets/)**.
