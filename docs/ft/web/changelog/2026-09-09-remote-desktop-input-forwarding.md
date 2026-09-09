# 2026-09-09 — a remote desktop you can actually use

A desktop opened in the browser now takes input. Move the mouse and type, and the remote machine
responds — on a host's desktop and on a session's alike, since both open the same overlay.

Clicks land where you aimed at any window size, and keep doing so when you resize the window
mid-session. The keys a desktop is unusable without — Enter, Tab, the arrows, the function keys,
Backspace and Delete, and the modifiers held down — all arrive as the desktop expects them.

**Escape now goes to the desktop.** It used to close the overlay, which put every full-screen
application on the far side out of reach: no leaving vim's insert mode, no dismissing a dialog over
there. The way out from the keyboard is `Ctrl+Alt+Esc`, and the overlay says so on screen, because
every other key now leaves the browser. The close button and clicking outside the picture are
unchanged. Combinations the browser reserves for itself — `Cmd+W`, `F5`, `Cmd+Tab` — can never be
captured by a web page and do not reach the desktop.

Right-clicking opens the remote machine's context menu without the browser's own menu appearing on
top of it. Whatever you are holding when you close the desktop is released on the way out, so the
exit chord does not leave the far side stuck in a Ctrl+Alt state.

A desktop whose input stream cannot be opened keeps its picture and says input is unavailable, so it
can still be watched. One thing that notice does not cover: a bridge that cannot inject input drops
it silently instead of refusing, so a genuinely view-only server is ignored rather than reported.

Not forwarded yet: the scroll wheel, audio, clipboard and file transfer.
