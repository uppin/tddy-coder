# 2026-11-07 — Storybook moved to `tddy-storybook` (#web-split 19/20, PR #852)

`.storybook/` and every `*.stories.tsx` left this package; `App.stories.tsx` was deleted. The
`storybook` and `build-storybook` scripts delegate to `tddy-storybook`, the Storybook devDependencies
went with it, and `cypress:e2e*` / `run-cypress-serve.sh` serve `../tddy-storybook/storybook-static`.
