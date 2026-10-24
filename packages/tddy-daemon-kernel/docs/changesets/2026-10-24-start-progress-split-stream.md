# 2026-10-24 — Split start streams, no deadline

**Type:** Feature

`#start-progress` 4/8 ([#737](https://github.com/uppin/tddy-coder/pull/737)). `forward_start_session_relaying` + `ForwardedStartFailure::{PeerAnswered, PeerSilent}`; `stream_relay::RelayFailure::{Peer, Silent}` and `CommonRoom::forward_server_stream_classified`; `forward_start_session_within` deleted. See [daemon kernel](../daemon-kernel.md). Cross-package entry: [docs/dev/changesets/2026-10-24-start-progress-split-stream.md](../../../../docs/dev/changesets/2026-10-24-start-progress-split-stream.md).
