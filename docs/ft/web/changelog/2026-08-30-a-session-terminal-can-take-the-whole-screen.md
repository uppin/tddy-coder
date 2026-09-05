# 2026-08-30 — A session terminal can take the whole screen

- **Every terminal pane in the session view now has a ⛶ full-screen control**, at the right end of the tab strip, acting on whichever pane is showing — Agent, a bash tab, a spawned conversation, or a conversation with an attached agent.
- **What goes full screen is the pane stack, not the single pane.** Only one pane is ever visible, so the operator sees exactly the active terminal — and the "Claim terminal" mutex overlay and the LiveKit connection overlay ride along. Handing over the pane alone would have left a session whose control another screen holds looking interactive while it swallowed every keystroke.
- **The control sits outside the tabs' horizontal scroller**, so a session with a dozen terminals cannot push it out of reach.
- **The tab strip is left behind** — full screen is the whole viewport for one terminal — so the pane draws its own exit control while it holds fullscreen, rather than stranding the operator on <kbd>Esc</kbd>.
- **Nothing unmounts across the transition.** Every terminal of the session keeps streaming and the session keeps its control lease; the grid re-fits itself through the terminal's existing `ResizeObserver`.
- **Not in the URL, unlike the inspector's `?full=1`:** the Fullscreen API needs a user gesture, so a shared link that claimed to reproduce the mode would silently not.
- Known limitation: <kbd>Esc</kbd> exits full screen before the terminal sees it, so a full-screen `vim` cannot be left with <kbd>Esc</kbd> alone — that needs the Keyboard Lock API, which changes the exit gesture to press-and-hold.
- See [session-terminal-tabs.md](../session-terminal-tabs.md) § Full screen.
