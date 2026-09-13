# 2026-09-10 — The Docker-backed LiveKit suites contend across test binaries

**Category:** Known failing test
**Source:** `#unbundle` node 4, [#473](https://github.com/uppin/tddy-coder/pull/473)

Four suites in `tddy-daemon-livekit` drive a real LiveKit server through the Docker testkit:
`session_room_livekit_acceptance`, `livekit_peer_daemons_acceptance`,
`common_room_duplicate_identity_repro` and `common_room_set_metadata_handshake_repro`.

They are `#[serial]` **within** a binary, but cargo runs test binaries in parallel, and `#[serial]`
does not span binaries. Inside `tddy-daemon` they were four of twenty-five suites, so they rarely
started together; in a crate with eight they do. `cargo test -p tddy-daemon-livekit` therefore
starts them close together against **one shared LiveKit container**, and
`session_room_livekit_acceptance` lost a test to that on one run of three.

- Run alone: **6 passed / 0 failed.**
- `--test-threads=1` across all four: **11 passed / 0 failed.**

This is a harness property, not a behaviour one — the code moved verbatim. The fix is a
`serial_test` group that spans the crate rather than a binary, or a testkit that hands each binary
its own container. Until then the crate's README and
[`livekit-service.md`](../../../packages/tddy-daemon-livekit/docs/livekit-service.md) both document
`--test-threads=1` as the reliable invocation.

**Related, and often mistaken for this:** nothing *skips* when Docker is absent.
`LiveKitTestkit::start()` returns `Err` and every caller `.expect()`s it, so these suites **fail**
without `/var/run/docker.sock`, naming `LIVEKIT_TESTKIT_WS_URL` as the alternative. Which also means
the real LiveKit join path is unexercised wherever Docker is missing — see
[2026-09-05-from-2026-09-05-tauri-desktop-single-process-daemon.md](./2026-09-05-from-2026-09-05-tauri-desktop-single-process-daemon.md).
