# 2026-10-25 — Files dropped on a session terminal in Tddy Desktop, and drops pasted

- **Tddy Desktop:** a file dragged from the OS onto a session terminal highlights the terminal under
  the pointer and, on drop, types the file's **real path** into a session on this machine whose agent
  reads its files; any other session gets the file **uploaded** and the uploaded path typed. A folder
  is refused with a visible message.
- **Every drop is delivered as a paste**, bracketed when the program enabled bracketed paste — so
  Claude Code attaches a dropped image path as an image, in a browser as well as in Desktop.

See [web-terminal.md § File drop upload — In Tddy Desktop](../web-terminal.md#file-drop-in-tddy-desktop).
