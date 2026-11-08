# Session package (`tddy-session-web`)

**Package:** `packages/tddy-session-web`
**Applies to:** the sessions drawer, session runtime, main pane, inspector and agent layers
**Added:** 2026-11-07 (#web-split 15/17, PR #851) — seams and helpers; the runtime and terminals
2026-11-07 (#web-split 16/17, PR #860); the inspector, agents and code pane 2026-11-07
(#web-split 17/17, PR #861); the drawer, project home and status strip 2026-11-07
(#web-split 18/20, PR #862)

## Overview

Session code becomes one package that does not depend on the web app. It reaches what the app owns —
the location, the screen shell, the passphrase prompt a cloning start raises — only through slots the
app fills, and holds the session helpers every session surface uses.

## Behaviour

- Sessions behave as before ([session-drawer.md](session-drawer.md)); the passphrase prompt is
  provided once for the whole app.

## Acceptance criteria

- [x] Session helpers live in `tddy-session-web`, which imports nothing from `tddy-web` or LiveKit (rule test).
- [x] Session code reaches the location, the shell and the passphrase prompt only through slots (seam rules).
- [x] The app's session screens render as before (`tddy-web`'s session specs pass).
- [x] The session runtime and terminals live in the package; their specs run on port doubles, not LiveKit.
- [x] The inspector, agent layers, code pane and main pane live in the package; their specs pass from it.
- [x] The drawer and project home live in the package; the web app holds no session component.

Technical reference: [packages/tddy-session-web/docs/README.md](../../../packages/tddy-session-web/docs/README.md).
