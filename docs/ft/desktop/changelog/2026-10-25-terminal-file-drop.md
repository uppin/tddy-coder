# 2026-10-25 — Drop files from Finder onto a session terminal in Tddy Desktop

- **Dropping files on a session terminal works in Desktop.** It did nothing before: the window's
  native drag-drop handler took every drag from the OS and the terminal never saw it. The terminal
  under the pointer is highlighted while files are dragged over it.
- **A session on this Mac that is not confined gets the files' real paths**, shell-quoted and
  space-separated with a trailing space — what a native terminal types for a drop. They arrive as a
  paste, bracketed when the program asked for it, so Claude Code attaches a dropped image.
- **Every other session gets the files uploaded** into its session directory and the uploaded paths
  typed — a session on another host, or one on this Mac that is jailed, managed-codebase, split from
  its checkout, or running its tools over SSH.
- **A dropped folder is refused with a visible message**; the files dropped with it still go
  through.
- Known gap: in-page drags (a Files-tab entry onto the terminal) and Finder drops on the
  create-session attachments area are still swallowed by the window in Desktop.

See [web-terminal.md § File drop upload — In Tddy Desktop](../../web/web-terminal.md#file-drop-in-tddy-desktop).
