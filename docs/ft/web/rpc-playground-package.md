# RPC playground package (`tddy-rpc-playground-web`)

**Package:** `packages/tddy-rpc-playground-web`
**Applies to:** the RPC playground screen (`#/rpc-playground`)
**Added:** 2026-11-07 (#web-split 9/17, PR #846)

## Overview

The RPC playground screen — reflection-driven service browser and invoker — is its own package: the
screen, the reflection registry, invocation over a raw transport and the protobuf-JSON helpers. It
reaches daemons only through `tddy-client-web` and renders with `tddy-ui-web`; it does not depend on
the web app or on LiveKit. The app page that places it in the shell, with sign-in and host presence,
stays in the web app.

## Behaviour

- Playground behaviour is as before ([rpc-playground.md](../daemon/rpc-playground.md)), including its
  URL state ([url-state-routing.md](url-state-routing.md)).

## Acceptance criteria

- [x] The playground screen lives in `tddy-rpc-playground-web` and imports nothing from `tddy-web` or LiveKit (rule test).
- [x] Its unit suites and Cypress specs pass from the package (`web.yml` runs them).
- [x] The playground page works as before (`tddy-web` builds and its menu-entry spec passes).

Technical reference: [packages/tddy-rpc-playground-web/docs/README.md](../../../packages/tddy-rpc-playground-web/docs/README.md).
