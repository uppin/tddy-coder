# 2026-10-23 — Cypress component tests run without the React Compiler, so compiler-only bugs pass every test

**Category:** Known test gap
**Source:** `web-dev` LiveKit and visible errors (#706)

## What happens

`packages/tddy-web/vite.config.ts` enables `babel-plugin-react-compiler` unless
`CYPRESS_DISABLE_REACT_COMPILER=1`, and `cypress.config.ts` always sets that variable. Development
and production builds therefore run compiled, while every component test runs uncompiled.

In #706, `useHostConnector` and `SelectedDaemonScope` each keyed a `useMemo` on a value its body
never read. Under the compiler that key was dropped, and no RPC client reached any host once the
common room joined after the first render. All 1073 component tests passed throughout. Only a
Chromium run against a live `./web-dev` showed it. The fix (`"use no memo"`) is in, but nothing
automated would catch the next instance.

## Why not fixed in #706

The comment in `vite.config.ts` says the compiler breaks the imperative refs and predictable state
some component tests rely on. Turning it on for the suite is a separate effort, with an unknown
number of specs to fix.

## What would close it

Any of these would:
- run a small compiled smoke suite, for example a Cypress e2e or Playwright run against a `vite build`
  output, covering host routing: a room joins after the first paint, then a client exists and an RPC
  is sent;
- enable the compiler for a tagged subset of component specs;
- add a lint rule that rejects a `useMemo`/`useCallback` dependency the body never reads unless the
  function says `"use no memo"`.
