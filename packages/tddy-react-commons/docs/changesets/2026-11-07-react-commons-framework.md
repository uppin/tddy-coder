# 2026-11-07 — Composable React framework package (#web-split 2/16, PR #839)

**Type:** Feature

New workspace package `tddy-react-commons`: events, stores, container, ports, async and testkit
modules, exported by subpath (the root export is empty; import from `./events`, `./stores`,
`./container`, `./ports`, `./async`, `./testkit`). React 19 peer, runtime deps `rxjs` and `dequal`.
Exports map to `src/`, so consumers resolve it from source.

- 182 `bun test` tests across 28 files under a happy-dom preload (`test-utils/dom-preload.ts`);
  `tsc --noEmit` clean. Subpath resolution from another package verified by importing all six
  subpaths through a `node_modules` link.
- PR check `web / tddy-react-commons tests` from `.github/workflows/web-tddy-react-commons.yml`.
- `useStore` is a `Store` method (`store.useStore(selector?, equality?)`), not a standalone export.
- No consumer yet; later `#web-split` nodes declare their seams with it.
