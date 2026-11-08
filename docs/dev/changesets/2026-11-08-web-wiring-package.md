# 2026-11-08 — `tddy-web` becomes the wiring package (#web-split 20/20, PR #853)

**Type:** Architecture change

`tddy-web` now holds only composition: an app container (`tddy-react-commons`'s `ReactAppContainer`)
whose handler builds the app's ports, the port implementations, routing, the shell and the pages
placing each package's screen. A wiring rule test rejects any feature module under its `src/`; an
`AppComposition` spec routes every screen through the composed app with all ports doubled. The app's
provider ports have no fallback (developer, 2026-11-08): a missing port throws naming it.
Deviations from the plan: the handler builds the React-free ports only; `clients` and `auth` are still
built by the stateful providers in the tree (recorded:
`docs/dev/todo/2026-11-08-app-handler-builds-no-clients-or-auth-port.md`).
At wrap: rebased onto #852; the composed tree's leftover `HostStartPassphraseScope` wrapper (removed
from the app root in #851, and its export deleted) is gone, so the prompt has one provider; doc paths
follow `src/rpc/` and `src/hooks/` into `src/app/`.
Fixed at wrap: `tddy-ui-web/theme.css` scanned the component packages through a wildcard Tailwind does
not expand, so since #840 every class only a component package uses was missing from the app's
stylesheet (and the Storybook's). One `@source` per package now, pinned by `themeSources.test.ts`. The
two component specs the green recorded as failing on the base were symptoms of the same gap and of the
base's state: `CodeHoverCard` (its card had no `max-h-80`) and `SessionsSshExecAcceptance` both pass.
Package detail: [tddy-web](../../packages/tddy-web/docs/changesets/2026-11-08-web-wiring-package.md),
[tddy-ui-web](../../packages/tddy-ui-web/docs/changesets/2026-11-08-web-wiring-package.md).
Product description: [web-wiring-package.md](../ft/web/web-wiring-package.md).
