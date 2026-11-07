# tddy-storybook

The one Storybook for the tddy web packages. It owns **every** story — component packages carry none —
grouped by package under `src/<package>/`, and renders each against the port doubles from
`tddy-client-web/testkit`. `bun run storybook` serves it; `bun run build` writes `storybook-static/`,
which `tddy-web`'s e2e terminal specs run against. `bun test src` checks story placement and renders
every story.
