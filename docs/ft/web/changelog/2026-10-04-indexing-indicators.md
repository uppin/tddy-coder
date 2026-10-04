# 2026-10-04 — Start progress and code-index loading in the session UI

`#live-plan` 12/15, PR [#571](https://github.com/uppin/tddy-coder/pull/571).

Creating a session says what the host is doing: under the form, *Creating worktree…*, *Indexing
(semantic)…* and *Starting agent…* follow the start's steps, with or without attachments, with the Create button disabled meanwhile. Once the session exists, its header shows *Indexing — `<phase>` `<n>`%*
while the code index loads in the background, nothing once it is ready, and *Indexing failed — `<reason>`*
if the load fails — the session stays usable throughout, and navigation answers whenever the index can.
A host without a configured index, or a worktree that is not a Rust workspace, shows no indicator.
Sandboxed, tool and split sessions, and children spawned by a PR-stack orchestrator or a grill-me
conversation, report no start steps and load their index on their first navigation request. See
[Session Drawer § Start progress](../session-drawer.md#start-progress) and
[Worktree Code Pane § Indexing indicator](../session-code-pane.md#indexing-indicator).
