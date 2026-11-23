# Create-session package (`tddy-session-create-web`)

**Package:** `packages/tddy-session-create-web`
**Applies to:** every surface that starts a session — the drawer's create pane, the PR-stack screen's
Start-session dialog, project-home setup
**Added:** 2026-11-07 (#web-split 7/17, PR #844)

## Overview

The create-session experience is one package: the form, its fields and sections, the request it
sends, attachment staging and the start progress list. It reaches daemons only through
`tddy-client-web` and renders with `tddy-ui-web`; it does not depend on the web app or on LiveKit.
The web app mounts it and supplies the one thing it cannot own: the Hosts screen's passphrase dialog,
shown while a start clones over SSH.

## Behaviour

- Every create-session behaviour, field and request is as before
  ([session-agent-catalog-fan-out.md](session-agent-catalog-fan-out.md),
  [tool-session-model-selection.md](tool-session-model-selection.md),
  [web-workspace-setup.md](web-workspace-setup.md)).
- A start that asks for a key passphrase shows the Hosts screen's dialog over the form, as before.
- A **"With work queue"** checkbox (`create-session-with-work-queue`, unchecked by default) starts
  the session owning an empty work queue (`StartSessionRequest.with_work_queue`). A queue is created
  only at start, so a session started without it has none — and no Work queue tab
  ([session-drawer.md § Work queue tab](session-drawer.md#work-queue-tab)).
- The Host field groups the hosts a daemon reaches through a remote shell (its SSH config aliases
  and its Coder workspaces) under that daemon. Choosing one starts the session on the parent daemon with the derived host as
  its target, and the form says that permission prompts are answered in the terminal.
- A derived host that cannot be used — a Coder workspace that is not running, or one on a daemon
  without the `coder` CLI — is shown with its reason and cannot be chosen. The web reads this from
  a non-empty unusable reason, so a host carrying no reason stays choosable.
- Each daemon's derived hosts are listed on their own: one daemon that never answers withholds
  only its own hosts, an earlier answer is kept until a newer one arrives, and a daemon is asked
  again when it connects, when a Coder account is connected and when the vault is unlocked. A
  daemon whose listing failed says so beside the Host field — "Could not list the hosts <parent>
  reaches: <reason>" — while its own daemon entry stays offered.

## Acceptance criteria

- [x] Create-session lives in `tddy-session-create-web` and imports nothing from `tddy-web` or LiveKit (rule test).
- [x] Its specs pass from the package's own Cypress config and PR check.
- [x] `tddy-web` renders it unchanged (its drawer and PR-stack start-session specs pass).
- [x] A derived host is offered under its parent daemon, and starting on it sends the parent as
      `daemon_instance_id` with the derived host's target (`CreateSessionDerivedHostAcceptance`).
- [x] A Coder workspace is offered the same way with its Coder target, and a stopped workspace
      cannot be chosen (`CreateSessionCoderHostAcceptance`).
- [x] A parent's listing failure is shown beside the Host field and the daemon stays offered
      (`CreateSessionDerivedHostAcceptance`).
- [x] Checking "With work queue" starts the session with a queue (`CreateSessionWorkQueueOptionAcceptance`).

Technical reference: [packages/tddy-session-create-web/docs/README.md](../../../packages/tddy-session-create-web/docs/README.md).
