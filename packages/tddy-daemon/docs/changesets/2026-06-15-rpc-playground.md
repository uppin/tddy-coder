# 2026-06-15 — **RPC Playground

**Type:** Feature

daemon registration + LiveKit participant** — `reflection_entry_from()` registered in `main.rs` after `rpc_entries` assembly; daemon spawns a dedicated `LiveKitParticipant` in the common room (identity `daemon-{id}`) serving the full RPC set so the playground can discover and invoke via data channel. Feature [rpc-playground.md](../../../../docs/ft/daemon/rpc-playground.md); product [daemon/changelog/](../../../../docs/ft/daemon/changelog/). (tddy-daemon)
