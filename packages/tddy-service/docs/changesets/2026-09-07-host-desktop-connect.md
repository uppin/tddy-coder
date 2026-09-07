# 2026-09-07 — Host-scoped screen-sharing RPCs, and a desktop-password prompt kind

**Type:** Feature

Top node of the `#hosts-screen` stack ([#460](https://github.com/uppin/tddy-coder/pull/460)). See
the cross-package entry for the whole change:
[docs/dev/changesets/2026-09-07-host-desktop-connect.md](../../../../docs/dev/changesets/2026-09-07-host-desktop-connect.md).

`screen_sharing.proto` gains the host-scoped half of `ScreenSharingService`: `ListHostTargets`,
`AddHostTarget`, `StartHostStream` and `StopHostStream`, with their request and response messages.
They are addressed by `session_token` + `daemon_instance_id` + `target_id` where the session-scoped
calls take a `session_id`, and they reuse `Protocol` and `ScreenSharingTarget` rather than restating
them. `StartHostStream` returns the existing `StartStreamResponse`, because
`{ livekit_room, livekit_url, bridge_identity, track_name, width, height }` is already exactly what
the browser overlay consumes.

**`StartHostStreamRequest` deliberately carries no password field.** `HostPromptEvent` in
`connection.proto` is the only place a host publishes the public key an answer is encrypted under —
and the fingerprint a client pins — so a caller that has not been asked anything has nothing to
encrypt with. The password is therefore asked for on the prompt channel while the call is blocked,
and nothing on the host stores it.

`connection.proto` gains `HOST_PROMPT_KIND_DESKTOP_PASSWORD = 2` on `HostPromptKind`, additively:
the channel, its crypto, the host keypair lifecycle and the key-pinning rules are unchanged, and a
prompt that does not state a kind still reads as the ssh key passphrase. Two surfaces now share one
feed, so each states which question it means.

Regenerated for both languages.
