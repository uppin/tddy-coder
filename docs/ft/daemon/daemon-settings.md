# Daemon settings — design

## Purpose

The daemon's own configuration, readable and writable from its UI. Before this, changing a LiveKit
URL meant editing YAML and restarting; the desktop app is *a daemon with a UI*, so its settings
belong in that UI.

The **YAML file the daemon was started with stays the source of truth.** Nothing here introduces a
second store.

## The service

`daemon_config.DaemonConfigService` — `packages/tddy-service/proto/daemon_config.proto`, implemented
in `packages/tddy-daemon/src/daemon_config_service.rs`, with the rules themselves as pure functions
in `daemon_settings.rs`.

| Method | Gated | Behaviour |
|---|---|---|
| `GetConfig` | yes | the effective configuration, **secrets redacted** — an API secret is reported as *set*, never returned — plus the path of the file an update writes |
| `UpdateConfig` | yes | validate → write that file → apply what can be applied live → name what cannot |
| `GetClientConfig` | **no** | the payload a browser gets unauthenticated at `GET /api/config` |

`GetClientConfig` is deliberately ungated: it is read *before* sign-in, because it is what tells a
page there is a daemon to sign in to. It carries a LiveKit URL and room name, the agent allowlist,
the debug mask and the instance id — no secrets. A gate there makes a desktop webview unable to
bootstrap at all.

## What an update means

An update carries the **complete** settings message; a partial one would be indistinguishable from a
request to clear the fields it omits. From that:

- **Secrets**: an omitted `api_secret` leaves the stored one alone — the UI never held it to send
  back. A supplied one replaces it. The form must therefore send *nothing* rather than `""`.
- **Validation happens before any write.** A refused update leaves the file byte-for-byte unchanged.
  A LiveKit URL must be `ws://` or `wss://`.
- **Writes are atomic** — a temp file in the same directory, then a rename, carrying the target's
  permissions — so a failed write can never truncate an operator's config.
- **What cannot be applied live is named**, not dropped: `restart_required` lists field paths such as
  `listen.web_port`. The UI shows them.

## Runtime reconfiguration

Only the LiveKit block applies live. A supervisor owns the running common-room connection and, when
the URL or the room name changes, **tears the current one down before bringing the new one up**.

The failure mode is deliberate: a block missing any of the four fields it needs to be joinable never
becomes a target, so the daemon ends up **disconnected**, with a warning naming what was missing —
never silently still in the old room while the settings screen reports the new one.

## Known limits

- **Comments are lost.** Writing re-serializes the config, so an operator's YAML comments do not
  survive a save. Field values do — verified by round-tripping `dev.daemon.yaml`, `dev.desktop.yaml`
  and `config.example.yaml`. Preserving comments needs a comment-aware YAML editor.
- **Peer discovery does not follow a runtime reconnect.** A daemon that *gains* a common room at
  runtime serves its roster there but does not discover the other daemons in it until restarted.
- **The UI edits the LiveKit block only.** Everything else is read-only and reported as
  restart-required.

## Related docs

- [Tddy desktop app (Tauri)](../desktop/tddy-desktop-tauri.md)
