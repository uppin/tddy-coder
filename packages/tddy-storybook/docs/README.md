# tddy-storybook documentation

The one Storybook for the web packages. It owns every story — the component packages carry none
(`src/storyPlacement.test.ts`) — grouped by package under `src/<package>/`, one at least per component
package for its main screen or component.

| Path | Contents |
|---|---|
| `.storybook/main.ts` | Storybook 9 on `@storybook/react-vite`; Tailwind; the shared pre-bundle list (`scripts/vite-prebundle.ts`) so the dev server never re-optimises mid-run |
| `.storybook/preview.tsx` | every story mounted in the clients, navigation and auth ports (`withClients`) plus the presence, room-RPC and media doubles from `tddy-client-web/testkit` |
| `src/<package>/*.stories.tsx` | the stories; the moved ones keep explicit `title`s, so their ids are unchanged (`components-button--primary`, `components-terminal--*`) |
| `src/storiesRender.test.tsx` | renders every story with the preview's decorators (`setProjectAnnotations`); `TerminalView` needs a browser and is covered by the static build and `tddy-web`'s e2e specs |

`bun run storybook` serves it; `bun run build` (alias `build-storybook`, after a `prebuild` of
`tddy-livekit-rpc-web`) writes `storybook-static/`, which `tddy-web`'s e2e scripts
(`cypress:e2e*`, `run-cypress-serve.sh`) serve.

- **Changeset history**: [changesets/](changesets/)

## CI

`.github/workflows/storybook.yml` builds it (`STORYBOOK_PACKAGE: tddy-storybook`) when a change reaches
it; `web.yml` runs `bun test src` in a `Web tests` shard and its `typecheck` in `Web lint`.
