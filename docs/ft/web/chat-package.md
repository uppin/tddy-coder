# Chat package (`tddy-chat-web`)

**Package:** `packages/tddy-chat-web`
**Applies to:** every surface that shows an agent conversation — the session's agent chat and
activity views, the PR-stack chat, workflow chat, the models screen's chat
**Added:** 2026-11-07 (#web-split 10/17, PR #847)

## Overview

Agent chat is one package: the chat component and its views, the presenter and ACP streams behind one
result shape, transcript replay and tool-call detail, the agent activity registry and panes, and the
subagent conversation and permission UI. It reaches daemons only through `tddy-client-web` and renders
with `tddy-ui-web`; it does not depend on the web app or on LiveKit.

## Behaviour

- Chat, ACP streaming, elicitation, transcript export, activity and permission behaviour are as before
  ([session-drawer.md § Agent Chat](session-drawer.md), [acp-protobuf-rpc.md](../coder/acp-protobuf-rpc.md)).
- An exported transcript names a subagent bubble `Subagent` (it printed `undefined`).

## Acceptance criteria

- [x] Chat lives in `tddy-chat-web` and imports nothing from `tddy-web` or LiveKit (rule test).
- [x] Its specs pass from the package (`web.yml` runs them; `Web lint` typechecks it).
- [x] Sessions, PR-stack and models render chat unchanged (`tddy-web`'s chat-hosting specs pass).

Technical reference: [packages/tddy-chat-web/docs/README.md](../../../packages/tddy-chat-web/docs/README.md).
