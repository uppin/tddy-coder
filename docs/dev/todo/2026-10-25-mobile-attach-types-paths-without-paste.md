# 2026-10-25 — The mobile Attach button types uploaded paths without a paste

**Category:** Future enhancement
**Source:** #765 (Finder drops onto Desktop session terminals)

Dropped files' paths reach the PTY as a **paste** (`GhosttyTerminalSession`'s `pasteInput` →
ghostty's `paste`), so a program that enabled bracketed paste — Claude Code — receives them between
`ESC [ 200 ~` and `ESC [ 201 ~`, which is how it attaches a pasted image path as an image. The mobile
**Attach** button (`TerminalUploadButton`) still inserts the uploaded paths with `sendInput`, as
plain typed input, so an image attached from a phone is typed rather than attached.

Not changed in #765 because it is a separate route that was not part of the reported defect.

**What would close it:** hand `TerminalUploadButton` the same `pasteInput` the drop zone gets, and
cover it in `MobileTerminalUploadButton.cy.tsx` the way `GhosttyTerminalSession.cy.tsx` covers a
dropped path.
