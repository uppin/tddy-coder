# 2026-10-18 — `Web tests` run LiveKit from the floating `:master` tag; everything else is pinned

**Category:** Future enhancement (CI hygiene)
**Source:** investigating [the `startEchoServer` timeout](2026-10-18-livekit-web-start-echo-server-intermittently-exceeds-its-task-timeout.md)

The e2e legs read their LiveKit image from `.config/livekit-server.image` (`livekit/livekit-server:v1.13.7`), the
single place it is pinned, and [ci.md](../guides/ci.md) says it is "never `:master`". `tddy-livekit-web`'s Cypress
testkit does not: `LIVEKIT_IMAGE` in `cypress/support/livekitDockerTestkit.ts` and the `Pull the LiveKit image for
tddy-livekit-web` step in `.github/workflows/ci.yml` both name `livekit/livekit-server:master`, kept in step by a
comment. ci.md records this as known.

A floating tag means the server under test can change between two runs of the same commit, so a LiveKit regression
would arrive as an unattributable red `Web tests`. No drift was seen on 2026-10-17: every run examined pulled the
same digest (`sha256:5fa6c11e…`), including the passing and the failing ones. The risk is latent, not observed.

## What would close it

Read the image from `.config/livekit-server.image` in the testkit (a `fs.readFileSync` next to the other constants)
and in the pull step, so the three places cannot disagree. Note the testkit's `docker run` flags
(`--dev --bind 0.0.0.0 --node-ip 127.0.0.1`) were written against `:master`; try them on `v1.13.7` first.

**Why deferred:** it is unrelated to the observed hang (same digest in passing and failing runs), and changing the
server version is its own risk. It is filed because it removes a variable from every future investigation here.
