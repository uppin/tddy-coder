# 2026-08-29 — the create-session pane stops withdrawing the agent picker and the Semantic index toggle on a split placement

**Type:** Fix

both were gated on `isSplitCodebase` and additionally **blanked at submit**, so the request did not equal what the form showed. The daemon now serves both on every placement (an agent is placed by where it runs, not gated by where the codebase lives), which leaves the two guards and the two submit-time blanks in `CreateSessionPane.tsx` offering nothing but a smaller feature set. Removed; the Recipe and Sandbox withdrawals stay, because those two fields are genuinely refused on a split. Four Cypress acceptance cases in `CreateSessionCodebaseHostAcceptance.cy.tsx` cover the picker and the toggle surviving a codebase-host selection and their values reaching the wire; the now-vacuous "restores the picker when the codebase comes back" case is deleted. Feature [session-agent-roster.md](../../../../docs/ft/daemon/session-agent-roster.md). (tddy-web)
