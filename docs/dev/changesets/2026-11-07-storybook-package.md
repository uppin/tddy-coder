# 2026-11-07 — `tddy-storybook`: one Storybook that owns every story (#web-split 19/20, PR #852)

**Type:** Architecture change

Adds `packages/tddy-storybook` (workspace entry, `bun.lock`): the Storybook 9 config moved out of
`tddy-web`, every story — the moved ones with unchanged ids, and one new story per component package
that had none — mounted in the port doubles, plus rules that no component package carries a story and
that every story renders. `storybook.yml` now builds this package; `web.yml` discovers it (caller
workflow deleted) and typechecks it. `tddy-web`'s e2e scripts serve its static build.
Also at wrap: `tddy-rpc-web`'s `createEnvelopeTransport` passes on the `log` option its options type
now declares (`tddy-tauri-web` passed one that was silently dropped), with a test — closing
`2026-11-07-tauri-web-transport-log-is-dropped.md`, so `tddy-tauri-web` and `tddy-hosts-web` leave
the `Web lint` exclusions and `tddy-terminal-web`'s count drops to 14.
Deviations from the plan: `App.stories.tsx` was deleted rather than moved (it rendered `tddy-web`'s
`App`); `ghostty-terminal.cy.ts` needs a built `tddy-demo` and LiveKit and runs in CI only.
Package detail: [tddy-storybook](../../packages/tddy-storybook/docs/changesets/2026-11-07-storybook-package.md),
[tddy-web](../../packages/tddy-web/docs/changesets/2026-11-07-storybook-package.md).
Product description: [storybook-package.md](../ft/web/storybook-package.md).
