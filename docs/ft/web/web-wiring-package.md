# Web wiring package (`tddy-web`)

**Package:** `packages/tddy-web`
**Applies to:** the page `tddy-daemon` serves and `tddy-desktop` embeds
**Added:** 2026-11-08 (#web-split 20/20, PR #853)

## Overview

After the split, the web app is a set of packages, and `tddy-web` composes them: an app container
builds the ports every package reaches the outside world through — navigation, presence, media and
room RPC, the desktop's native file drop — and the app's routes place each package's screen in the
shell. It holds no feature of its own.

## Behaviour

- Every screen, route and sign-in flow behaves as before.
- The app's stylesheet carries the classes every component package uses (the split had dropped those
  only a package used — overlays rendered without their backdrop, the code hover card without its
  height bound).
- A provider the app needs is never silently defaulted: one missing from the composition fails loudly,
  naming it.

## Acceptance criteria

- [x] `tddy-web` holds no feature module (wiring rule test).
- [x] Every route renders its package's screen through the composed app (`AppComposition` spec).
- [x] The app's routing, shell, sign-in and daemon-selection specs pass.

Technical reference: [packages/tddy-web/README.md](../../../packages/tddy-web/README.md).
