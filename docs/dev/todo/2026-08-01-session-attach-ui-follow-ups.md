# 2026-08-01 — Session attach UI — follow-ups

**Category:** Future enhancement
**Source:** session-attach-ui changeset, 2026-08-01

- **The browser→daemon leg has never run.** Nothing in the attach feature has been exercised against a
  real daemon: the daemon suites drive `ConnectionServiceImpl` directly, and every web Cypress spec
  stubs the RPCs with the in-memory backend. Real chunk uploads over the LiveKit data channel, a real
  streamed `StartSession`, and a real cross-host fetch are all unverified end to end. **The manual
  check:** bring up two daemons via `./web-dev`, then (1) in **New session** with the Host selector on
  the connected daemon, attach one local file and one host document — confirm per-row progress advances
  from streamed events and both land in `{session_dir}/artifacts/attachments/` before the agent's first
  turn; (2) repeat with the Host selector on the **second** daemon, which exercises the cross-host
  staged fetch and the streamed forward — the two paths most likely to hang silently rather than fail;
  (3) restart the staging host and confirm the staging root is gone.
- **Two files sharing a `File.name` in one batch fail opaquely.** They stage under the same
  `(staging_id, file_name)`; uploads are sequential, so the first writes its `.staged-complete` marker
  and the daemon refuses the second with *"staged file already exists in this batch"* — an error, not
  truncation or corruption. But it surfaces as an opaque daemon failure late in the submit instead of a
  form-level refusal beside the offending row. The duplicate-basename check catches the default case
  (a row's basename starts as its file name) but not a batch where the operator renamed one of two
  same-named files. The fix is a **unique staged file name per row**, not a second refusal.
  `TODO` at `packages/tddy-web/src/hooks/useStagedAttachmentUpload.ts`.
- **No consumed-batch staging GC.** The restart-cleared staging root
  (`std::env::temp_dir()/tddy-staging`) bounds *abandoned* batches, but a batch that a `StartSession`
  successfully consumed is still left on disk until the next host restart.
- **`RpcRequest.abort` is never read by `ServerEngine`.** Dropping a stream receiver does not stop the
  producer, so a peer keeps producing frames nobody drains and a client that disconnects mid-creation
  still gets its session created (an orphan the UI never showed). Unary `StartSession` has the same
  property — this is parity, not a regression — but streaming plus a multi-megabyte upload widens the
  window from seconds to minutes. A real fix needs an abort frame honoured server-side plus
  peer-disconnect teardown.
- **`StreamSessionActivity` / `StreamAcpReplay` / `WatchTask` / `WatchTaskList` still refuse
  `PeerRoute::Forward`.** The streaming-forward primitive they were blocked on now exists
  (`livekit_peer_discovery::forward_server_stream_to_peer`), but its idle deadline is sized for a
  short-lived stream and these four are open-ended, so migrating them needs a keepalive frame first.
  Their `TODO`s in `connection_service.rs` state this.
- **Do not "fix" the relay channel by bounding it.** `forward_server_stream_to_peer` uses an
  *unbounded* channel on purpose. The transport's bounded `mpsc::channel(32)` beneath it is filled by
  the room's **shared** response loop via an awaited send, so a relay that stopped draining would block
  that single loop and head-of-line-block *every other in-flight forwarded RPC on the daemon's
  common-room connection*. Buffering one stream is strictly better than stalling all of them. What
  bounds the buffer today is that both callers cap what they accept and both streams are short-lived; a
  real fix is per-stream flow control in the transport.
- **`cypress/support/livekit/fakeCommonRoom.ts` does not serialize `max_attachment_bytes`.** A Cypress
  `DaemonHost` fixture driven through the fake room therefore advertises no cap. No current spec needs
  it — the attachment specs inject hosts directly into `SelectedDaemonProvider` — but a future spec
  routing through the fake room would find the cap mysteriously absent.
- **`attachment_size_bytes` reports 0 when `metadata` fails** on a file it just wrote successfully
  (`connection_service.rs`). Display-only: it feeds the progress event's `bytes_total`, not a
  correctness gate.
- **`on_disk_size_bytes` reports 0 for a tracked-but-deleted file** (`worktree_files.rs`), logging a
  warning rather than skipping the entry — skipping would silently drop the file from the Code pane
  tree. It opens no cap-enforcement hole, since `stat` only fails when there are no bytes to attach, so
  a large file can never be understated. If the listing should instead drop unstattable paths, that is
  a small follow-up.
- **The three attachment Cypress specs each rebuild a near-identical baseline backend.** Real
  duplication, but 12 pre-existing `CreateSession*.cy.tsx` files duplicate it the same way, so the new
  specs followed the house pattern rather than inventing a second one. A shared
  `aCreateSessionBackend()` fixture is a suite-wide follow-up, not this feature's debt.
- **`CreateSessionPane.cy.tsx` fails spuriously on its first batched Cypress load.** It is the only
  `CreateSession*` spec still using `cy.intercept` against a real ConnectRPC transport (the rest use
  the in-memory backend); observed failing as a single 275 ms failure inside a 6-spec batch, then
  passing 29/29 alone and 63/63 on an identical re-run. Migrating it to `anInMemoryRpcBackend` would
  remove the flake.
