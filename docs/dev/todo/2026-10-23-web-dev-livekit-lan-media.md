# 2026-10-23 — `./web-dev`'s LiveKit container keeps media on loopback, so a LAN browser gets no media

**Category:** Deferred from `web-dev` LiveKit and visible errors (#706)
**Source:** `scripts/dev-livekit.sh` (`dev_livekit_start_container`, the `--node-ip 127.0.0.1` TODO)

## What happens

The container `./web-dev` starts runs `livekit-server --dev --node-ip 127.0.0.1`. LiveKit puts that address into its
ICE candidates. A browser on the same machine connects fine. A browser on another machine, opening a LAN `VITE_URL`
(`http://192.168.x.y:<port>`), reaches signalling through the published port but cannot connect media, because the
candidates name loopback.

## Why deferred

#706 is about `./web-dev` starting and stopping its own LiveKit and surfacing failures. LAN use is a separate setup
and nobody reported it broken. Before #706 it relied on an external LiveKit from `.env`, which still works: set all
of `LIVEKIT_URL` / `LIVEKIT_API_KEY` / `LIVEKIT_API_SECRET` and no container is started.

## What would close it

Derive `--node-ip` from `VITE_URL`'s host when it is not a loopback name (or accept `DEV_LIVEKIT_NODE_IP`), and set
`LIVEKIT_PUBLIC_URL` to the same host. Add a `dev_livekit_script.rs` case asserting the `docker run` arguments for a
LAN `VITE_URL`.
