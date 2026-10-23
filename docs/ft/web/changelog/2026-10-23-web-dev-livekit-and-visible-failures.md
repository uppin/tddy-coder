# 2026-10-23 — `./web-dev` runs its own LiveKit; the web UI says why an action failed

- **`./web-dev` starts its own LiveKit server in Docker.** It uses ports from the ephemeral range,
  stable per checkout, and removes the container on exit. An existing server is reused when `.env`
  names it completely. Ports already in use are reported, never killed. A run that died is cleaned up
  by the next run. See [local-web-dev.md](../local-web-dev.md).
- **Projects and the session screens show the daemon's reason next to the control that failed.** A
  host that nothing can reach blocks the screen with its name and the reason, while the header stays
  usable.
- **Fixed:** with the React Compiler on, which covers dev and production builds, the dashboard could
  reach no host at all after the common room joined.
