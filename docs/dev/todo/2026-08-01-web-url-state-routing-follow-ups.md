# 2026-08-01 — Web URL state routing — follow-ups

**Category:** Future enhancement
**Source:** web-url-state-routing changeset, 2026-08-01

- **Two worktrees cannot run `cypress:component` at once** — `vite.config.ts` sets no `server.port`,
  so every checkout's Cypress component dev server takes Vite's default 5173. A second concurrent
  run makes one checkout fetch the other's `cypress/support/component.ts`, and every spec after that
  dies with `Failed to fetch dynamically imported module` — which reads like a test failure but is a
  port collision. A `server.port: Number(process.env.VITE_PORT) || undefined` in `vite.config.ts`
  (mirroring the `dev` script's existing `VITE_PORT` convention) would let each worktree pick its
  own; left out of this changeset as unrelated shared-config scope.
- **`src/rpc/**` is in no `bun test` path** — `package.json`'s `test:unit` covers
  `src/components/connection src/components/sessions src/lib src/hooks src/utils`, and
  `bun test src/routing` covers routing. `src/rpc/selectedDaemon.test.ts` therefore never runs, and
  when this changeset touched it, it turned out it *cannot* run: it imports a `.tsx` module, so bun
  fails on `react/jsx-dev-runtime`. This changeset relocates those five cases to
  `src/routing/selectedHost.test.ts`, but the general gap stands — adding `src/rpc` to `test:unit`
  needs a bun-side JSX runtime (or every `src/rpc` unit test kept to pure `.ts` modules) first.
- **Migrate the hash router (`#/sessions/:id`) to real `history.pushState` paths** — deliberately
  out of scope here: real paths need server rewrite rules on the daemon's static bundle handler, and
  the hash grammar deep-links fine without them. Worth revisiting if the app ever wants distinct
  server-rendered entry points.
- **`WorktreesAppPage`'s host `<select>` (`daemonId`) is not in the URL** — it is a create-worktree
  form field rather than a destination, so it was left as local state. If the worktrees screen ever
  starts *listing* per host (today the list is local-daemon-only, per the note in that file), it
  becomes a navigable selection and should join the URL grammar.
- **The RPC Playground's `participant` param has no acceptance test** — `RpcPlaygroundAppPage`'s
  harness is `cy.intercept`-driven around LiveKit reflection; standing it up was disproportionate to
  the one param. Service/method are covered at the `RpcPlaygroundScreen` level.
