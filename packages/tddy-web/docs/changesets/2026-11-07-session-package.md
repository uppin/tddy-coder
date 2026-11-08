# 2026-11-07 — Session seams and helpers moved to `tddy-session-web` (#web-split 15/17, PR #851)

Session components read the location through `tddy-client-web`'s `useAppLocation` and the route
builders now in `tddy-client-web` (`routing/appRoutes` re-exports them). `AppClientsPorts` fills two
slots for every subtree: `SessionShellProvider` with `AppShell`, and `StartPassphraseProvider` with
`HostStartPassphrasePrompt`. `HostStartPassphraseScope` is deleted: its call sites no longer wrap, and
at wrap the root mount the stack cascade had restored in `index.tsx` was removed, so the prompt has one
provider. The session helpers left `lib/`, `utils/` and `rpc/` for the package.
