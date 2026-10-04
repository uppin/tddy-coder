# Worktree Code Pane

**Route:** `#/sessions` (within `SessionsDrawerScreen`)
**Components:** `WorktreeCodePane`, `WorktreeFileTree`, `CodeBlock`, `CodeNavigationOverlays`, `RestructurePlanDialog` (`packages/tddy-web/src/components/session/`)
**Mount point:** `SessionMainPane` (`packages/tddy-web/src/components/sessions/`)

## Overview

Every session — regardless of type — can open a **Code pane** that **splits the main pane**: a
directory tree of the session's **worktree** files on the left of the pane, and a read-only
**file preview** on the right. It gives the operator a way to browse the actual code the agent is
working on, alongside the live chat or terminal, without leaving the session.

This generalizes the existing `SessionWorkflowFilesModal`, which is a modal limited to a fixed
4-file allowlist (`changeset.yaml`, `.session.yaml`, `PRD.md`, `TODO.md`) in the session's
*metadata* directory. The Code pane instead browses the session's **worktree** (the git checkout at
`SessionEntry.repo_path`), which holds the source files.

### Availability across session types

The toggle lives in `SessionMainPane` (next to the existing **Inspector** toggle), which wraps every
base view — the workflow chat (`WorkflowChatScreen`, `tool`/workflow sessions), the PR-Stack screen
(`recipe === "pr-stack"`), and the terminal runtimes (`claude-cli` / `cursor-cli`). Because the
toggle is above the base-view switch (not inside `resolveWorkflowView`, which returns `null` for
terminals), the Code pane is available to **all** session types.

## Layout

```
┌──────────────┬──────────────────────────────────────────────┐
│ SessionDrawer│  SessionMainPane                              │
│              │  [ Code ] [ Inspector ]        ← toggle row   │
│  ● feat-x    │ ┌─────────────────────┬──────────────────────┐│
│  ○ old       │ │ base view           ║  Worktree Code Pane  ││
│              │ │ (chat / terminal /  ║ ┌────────┬───────────┐││
│              │ │  pr-stack)          ║ │ tree   │  preview  │││
│              │ │                     ║ │ src/   │  (file    │││
│              │ │                     ║ │  a.rs  │  contents)│││
│              │ │              resize ║ │ README │           │││
│              │ └─────────────────────╨─┴────────┴───────────┘││
└──────────────┴──────────────────────────────────────────────┘
```

- Closed (default): the base view fills the main pane exactly as today.
- Open: the base view and the Code pane sit in a horizontal split with a **draggable divider**
  (`react-resizable-panels`). The base view (terminal runtimes especially) stays mounted throughout —
  toggling the pane never tears down a live terminal or chat.

## Directory tree

- **Lazy, per-directory.** The tree first lists the worktree root. Expanding a folder fetches that
  folder's immediate children on demand. This scales to real repositories.
- Entries are ordered **directories first, then files**, each alphabetical.
- **File scope:** the listing respects `.gitignore` and excludes `.git`. Ignored paths
  (`node_modules/`, `target/`, `.env`, …) never appear, so secrets and build junk are not exposed.

## File preview

- Selecting a file loads its contents (read-only) and renders it in the preview region
  (`data-testid="worktree-file-preview"`).
- Markdown (`.md`) renders as sanitized structured markup (`renderSimpleMarkdown`). Every other file
  is **syntax-highlighted** when its extension maps to a recognized language, and rendered as plain
  monospace text otherwise (`data-testid="worktree-code-highlight"` wraps the highlighted block).
- Highlighting is tokenized client-side with `react-syntax-highlighter` (`PrismLight`, registering
  only the languages we ship). The language is derived from the file path (`codeLanguageForPath`);
  an unrecognized or extensionless path falls back to plain monospace with no highlighting. The
  Prism theme follows the app's light/dark mode.
- Reads are size-capped; content beyond the cap is truncated (flagged in the response).

## Code navigation

A Rust (`.rs`) file in the preview is navigable. The answers come from the warm rust-analyzer index
([Warm code-intelligence daemon](../coder/warm-code-intelligence-daemon.md)), reached through the
session host's daemon, which authorises each request exactly as it authorises a file read.

- **Go to definition.** Ctrl-click (cmd-click on macOS) an identifier. The definition's file opens in the
  same pane, scrolled to the definition's line and marked as the target. When there are several
  definitions the pane lists them to pick from; when there are none it says so. A definition outside the
  worktree (a dependency, the standard library) is listed but cannot be opened.
- **Hover.** Resting the pointer on an identifier shows a card with the language server's hover text,
  typically the item's signature and docs. The card sits over the bottom of the preview and has a
  **Close** action.
- **References.** The hover card's **References** action lists every reference to the symbol, its
  declaration included, as `path:line` entries. Selecting one opens that file at that line.
- **Rust only.** Other files — markdown, TypeScript, JSON — render exactly as before, with no
  identifier handlers. The index serves Rust, and nothing falls back to another language server.
- **Failures are visible.** A failed lookup shows an inline notice naming the action. A daemon with no
  index configured, or whose index cannot start, therefore tells the operator rather than silently
  doing nothing. Without a navigation service on the session's host the pane is the plain read-only
  preview.

Positions are one-based lines and one-based **byte** columns throughout, so identifiers after
multi-byte characters resolve correctly. Locations are relative to the worktree root.

## Indexing indicator

When a session's worktree is ready and the host has a warm code-intelligence index configured, the host
loads that worktree's index in the background; the session is usable at once and navigation answers
whenever the index can. The **session header** (the row above the base view, left of the toggles)
shows the load:

- **Loading** — *Indexing — `<phase>` `<n>`%*, following the index's own phase and percentage.
- **Ready** — nothing. The indicator disappears.
- **Failed** — *Indexing failed — `<reason>`*. The session is unaffected.
- **Nothing warming** — nothing. A host without an `index_daemon:` section, a worktree that is not a
  Rust workspace (no `Cargo.toml` at its root), and a session whose start does not trigger a warm-up
  show no indicator at all.

Progress comes from the session's **owning host**, over the same client as code navigation, and only the
session's owner may watch it. "Ready" means the index has loaded the worktree's graph, not merely that
its server is running.

Not every session warms on start. Sandboxed, tool and split sessions, and children spawned by a
PR-stack orchestrator or a grill-me conversation, trigger no warm-up and show no indicator; their index
loads on the first navigation request.

## Restructure plans

A restructure plan is a `.jsonl` file whose first line is a plan header. Previewing one shows an
**Open as plan** entry above the preview; any other `.jsonl` file, such as an event log, does not. The
entry is offered only where the session's host serves code navigation.

Opening it shows a dialog with one row per operation of the plan, in plan order:

- the operation's **id**, its **kind** (`rename_symbol`, …), the **item and file** it anchors to, and
  the **group** it belongs to (blank when it stands alone);
- its **status** — pending, in flight, applied, failed or rolled back — kept current while the dialog is
  open;
- whether it **still points at existing code**. An operation whose item changed, is no longer found in
  its file, or was edited by another plan shows *stale — `<reason>`* in its row.

A stale operation disables **Run**, and the dialog names every stale operation with its reason. Otherwise
**Run** applies the whole plan through the warm index. Each row turns applied as its operation lands. If a
transactional group does not compile at its end, the run stops with the reason, and every operation of
that group shows **rolled back** while operations applied outside the group stay applied. Run is refused
while another run holds the worktree. Statuses read from the journal are exact for a plan run from its
start and approximate for one whose journal does not describe a prefix of the plan.

The plan's operations come from the plan file in the worktree, which is read under the same rules as any
other file of the pane, so a plan can be opened only from a listed worktree of the session's project. The
plan is loaded, checked and applied by the [warm code-intelligence daemon](../coder/warm-code-intelligence-daemon.md);
nothing lists plans by scanning the worktree — the entry point is previewing the plan file.

## Backend contract

Two new **`ConnectionService`** RPCs, rooted at the worktree and secured like `RemoveWorktree`
(validate the `worktree_path` is in the project's `git worktree list`, canonicalize, and contain all
reads under the worktree root):

- `ListWorktreeDirectory(session_token, project_id, worktree_path, rel_path)` →
  `entries: WorktreeDirEntry{ name, is_dir }` for the single directory level at `rel_path`
  (empty = root), `.gitignore`-filtered and `.git`-excluded, directories first.
- `ReadWorktreeFile(session_token, project_id, worktree_path, rel_path)` →
  `content_utf8`, `truncated`, `byte_size`.

Navigation uses a third service, **`code_navigation.CodeNavigationService`**
(`Definition`, `References`, `Hover`; `WatchCodeIndex(session_token, session_id)`, the indexing indicator's stream; and `OpenPlan`, `WatchPlan` and `RunPlan`, the plan dialog's calls), keyed by the same `session_token`, `project_id` and
`worktree_path` plus the file's `rel_path` and a position. It authorises the worktree through the same
listed-worktree check as the file RPCs, rejects a `rel_path` that leaves the worktree, and answers
`FAILED_PRECONDITION` naming the `index_daemon:` configuration section when the daemon has none.
Contract: [`code_navigation.proto`](../../../packages/tddy-service/docs/code-navigation-proto.md); how it is
served: [code navigation service](../../../packages/tddy-daemon/docs/code-navigation-service.md).

The client identifies the worktree by `SessionEntry.repo_path` (the worktree is bound to the
session). `rel_path` is validated to reject traversal (`..`), absolute paths, and any resolution
outside the worktree root.

## Acceptance criteria

1. **Available on every session type.** For a `claude-cli` (terminal) session, a `tool`/workflow
   (chat) session, and a `pr-stack` session, a **Code** toggle is present in the main pane; clicking
   it splits the screen and reveals the Code pane (`worktree-code-pane`) without unmounting the base
   view.
2. **Lazy tree.** The tree lists the worktree root (directories first); expanding a directory issues
   a `ListWorktreeDirectory` for that path and shows its children.
3. **File preview.** Selecting a file issues a `ReadWorktreeFile` and shows the contents in
   `worktree-file-preview`; a `.md` file renders sanitized markdown, a code file renders monospace.
4. **Toggle closes.** Clicking **Code** again collapses the pane back to the single base view.
5. **Ignored/secret files hidden.** `.git` and `.gitignore`d paths (e.g. `.env`, `node_modules/`)
   never appear in the tree (enforced server-side).
6. **Traversal rejected.** `rel_path` containing `..`, absolute paths, or paths resolving outside the
   worktree are rejected; an unlisted `worktree_path` is rejected.
7. **Syntax highlighting.** Selecting a code file whose extension maps to a recognized language
   (e.g. `.rs`, `.ts`, `.py`, `.json`, `.yaml`) renders it as tokenized, colored code in
   `worktree-code-highlight`; a file with no recognized extension (e.g. `LICENSE`) renders as plain
   monospace text with no highlight container. Highlighting never alters the file's text content.
8. **Go to definition.** Ctrl-click on an identifier in a `.rs` preview opens the definition's file
   scrolled to its line, with that line marked as the navigation target.
9. **Hover.** Resting on an identifier shows its type text in a hover card.
10. **References.** The hover card's **References** action lists the references; selecting one opens its
    file at that line.
11. **Rust only, authorised.** Non-`.rs` files offer no navigation. A `worktree_path` not listed for the
    project is rejected, and a daemon with no `index_daemon:` section answers `FAILED_PRECONDITION`
    naming it.
12. **Open as plan.** Previewing a plan file shows "Open as plan"; previewing any other `.jsonl` file does not.
13. **Operations listed.** The dialog lists every operation with its id, op, item/file, group and status.
14. **Stale operations.** An operation whose item changed shows "stale — item changed", and Run is disabled
    and names it.
15. **Run.** Run applies the plan; each operation's row turns applied as its event arrives, and a failing
    group shows its operations rolled back.
16. **Plans are authorised.** A `worktree_path` not listed for the project is refused by the plan calls.
