# 2026-10-18 — Code pane CodeSession: warm on open, live phases, held queries, underline, hover card
**Type:** Feature

- `useCodeSession` (+ `CodeSessionFollower`): one `CodeSession` per mounted pane; reopened with
  backoff (`streamReopening.ts`, shared with the header); refusals not retried.
- `CodeIndexStatusStrip` reads the session: phase + percentage + message, `+N more` with every phase in
  its tooltip, starting / no-index / reconnecting states; `untracked` removed.
- Navigation over the session: held hover card and notice (*Index loading — <phase> <n>%*), "No
  definition found" only from a loaded index; `useDefinitionLookup` / `useHoverQuery` /
  `useReferencesLookup` over `navigationLookup.ts`.
- `ProbeStore` (`codeProbes.ts`): probe per position, underline + pointer cursor, plain click navigates.
- `HoverCard` anchored beside its identifier, bounded, scrolling body, footer actions;
  `renderSimpleMarkdown` gains headings and fenced code.
- Header: `useCodeIndexProgress` re-watches `WatchCodeIndex` with backoff.
- `WorktreeCodePaneProps.sessionId` removed (`SessionMainPane.tsx` 675 → 674).
- Specs: `WorktreeCodePaneCodeSession`, `WorktreeCodePaneReconnect`, `CodeHoverCard`,
  `SessionIndexingIndicatorResubscribe` (new); `CodeIndexStatusStrip`, `WorktreeCodePaneNavigation`
  rewritten.

Docs: [code-navigation.md](../code-navigation.md),
[session-start-and-indexing-progress.md](../session-start-and-indexing-progress.md#indexing-indicator).
Cross-package entry: [2026-10-18-code-index-warm-on-demand.md](../../../../docs/dev/changesets/2026-10-18-code-index-warm-on-demand.md).
