# 2026-10-18 — The Code pane warms its index and shows rust-analyzer's progress live

- **Opening Code warms the worktree's index** for every session type, including after a host restart;
  the agent's first code-tool call warms it too. *Code index not warmed for this session* is gone.
- **Live progress**: the strip names the phase reported most recently with its percentage and message,
  adds *+N more* when several phases run, and lists them all in its tooltip. A non-Rust worktree reads
  *No code index for this worktree*; an index let go while the pane is open reads *Code index
  starting…* and is warmed again.
- **No premature answers**: hover, go to definition and references asked while the index loads show
  *Index loading — `<phase>` `<n>`%* and fill in by themselves once it is ready. *No definition found*
  appears only from a loaded index.
- **Navigable symbols are underlined** on hover with a pointer cursor; a plain click goes to the
  definition.
- **Bounded hover card** anchored beside the symbol (at most `min(40rem, pane − 2rem)` × `20rem`), with
  rendered markdown, a scrolling body and its actions always visible.
- **Reconnects**: a dropped code session is reopened with backoff (1 s → 30 s), saying *Reconnecting…*;
  queries waiting are asked again.
- **Session header** follows the worktree's index whoever started the load, never starts one, and
  watches again with backoff when its watch ends.

See [Session code pane](../session-code-pane.md#code-navigation) and
[Session drawer § Start progress](../session-drawer.md#start-progress).
