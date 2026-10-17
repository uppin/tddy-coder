# 2026-10-17 — Peer forwarding runs behind a transport port

- **No change for operators.** Forwarding to a peer daemon behaves as before, with the same
  `peer_forward_timeout_secs` deadline, the same stream idle rule and the same errors.
- **A daemon with no transport refuses a forward as `FAILED_PRECONDITION`.** It reports no peer
  connected, the same answer a daemon whose common room is down gives.
