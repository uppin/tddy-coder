# 2026-08-15 — remote-git-repo-over-livekit

**Type:** Feature

new `client_connect` module: `connect_client(url, token, target_identity, wait)` is the join + participant-wait + `LiveKitRpcClientFactory` sequence every rust→rust client needs, now existing **once** in the workspace instead of three times (`tddy-tools`' `pty_relay.rs` and `session_tool_client.rs` migrated onto it). The extraction is a bug fix rather than tidying: all three copies returned `()` from the wait's inner block, making an exhausted event channel indistinguishable from the participant arriving, so the caller was handed a client addressed at somebody who was not there and then hung with no error. (tddy-livekit)
