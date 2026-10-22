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
| `GetIntegrationStatus` | yes | the live state of each optional integration the daemon has wired — see [Integration status](#integration-status) |

`GetClientConfig` is deliberately ungated: it is read *before* sign-in, because it is what tells a
page there is a daemon to sign in to. It carries a LiveKit URL and room name, the agent allowlist,
the debug mask, the instance id and `auth_flow` — which GitHub sign-in this daemon serves
(`"redirect"`, `"device"`, or absent for none; see [session-auth.md](session-auth.md#minting-login))
— and no secrets. A gate there makes a desktop webview unable to
bootstrap at all.

## Integration status

An optional integration — LiveKit and Telegram — has one answer to "is it configured, is it up", and
`GetIntegrationStatus` returns it: a list with one entry per wired integration, ordered by id, each
with its label, its state and when it entered that state. The call is gated like `GetConfig`, and is
served by the daemon itself rather than by the integration, so it stays readable when the
integration is the thing that is down.

| State | Means |
|---|---|
| `Disabled` | Switched off, or not configured at all. |
| `Misconfigured` | Configured with a required field missing; the entry names the missing fields (for LiveKit, any of `url`, `api_key`, `api_secret`; for Telegram, `bot_token`). |
| `Connecting` | Configured completely; nothing has answered yet. |
| `Connected` | The integration is up: LiveKit's common room is joined, or a session room has been opened; Telegram's bot was recognised, or a message was delivered. |
| `Unreachable` | Configured and not answering; the entry carries the last failure. |

LiveKit reports from two places: the common-room connection (connecting, joined, failed or lost,
with retries keeping the moment the outage began) and every session-room open. A room that could not
be opened because the LiveKit **server** could not be reached makes the entry `Unreachable`; a room
that failed for a reason that is the session's own — its checkout is gone, its terminal could not be
exposed — leaves the entry alone. A daemon with no LiveKit configured reports `Disabled`, never an
error. A **local session attach does not depend on any of this**: it never waits on, or fails with,
an unreachable LiveKit; the failure appears here instead ([Session rooms](session-room.md)).

Telegram reports as `telegram`, labelled **Telegram**:

| Config / event | State |
|---|---|
| no `telegram:` block, or `enabled: false` | `Disabled` |
| `enabled: true` with an empty `bot_token` | `Misconfigured` — missing `bot_token`; the daemon starts no bot |
| the bot is starting and its token has not been checked yet | `Connecting` |
| the bot's identity check succeeded, or a send succeeded | `Connected` |
| the identity check failed, or a send failed | `Unreachable` — the last error |

The daemon checks the bot's identity with Telegram before it starts listening for chat messages, so
a revoked token shows as `Unreachable` before any message is due. **No reported Telegram error
contains the bot token**: Telegram's API addresses embed it, and it is replaced with `<bot-token>`
before the error is recorded. The `telegram:` block gains no field for any of this.

### On the settings screen

The LiveKit block of the settings screen shows the `livekit` entry as one line: **Off**,
**Missing: url, api_secret**, **Connecting…**, **Connected · 3m** or **Unreachable · 12s: <error>**,
the duration being how long the integration has been in that state. A **Telegram** block below it
shows the `telegram` entry the same way (**Off**, **Missing: bot_token**, **Connecting…**,
**Connected · 3m**, **Unreachable · 12s: <error>**); Telegram is configured in the YAML file, so the
block has no fields. A daemon that reports no `telegram` entry shows no Telegram block. The screen
reads the status when it opens, every 5 seconds while it stays open, and right after a successful save, so an operator who
saves a URL sees whether the daemon reached it. A daemon that answers `Unimplemented` (an older one)
gets no line and is not asked again; any other failed read shows **Status unavailable: <message>**
on the LiveKit line, drops the Telegram block (whether the daemon has a Telegram entry is then
unknown), and never lands in the save error. Implementation:
[daemon-settings-screen.md](../../../packages/tddy-web/docs/daemon-settings-screen.md).

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

## The common-room switch

**`livekit.enabled` decides whether this daemon joins its common room at all**, and the settings
screen renders it as a checkbox above the LiveKit fields. It **defaults to `false`**, so the common
room is opt-in: a block carrying url, key, secret and room names a room the operator *could* join,
not one the daemon does.

Switching it off preserves everything else in the block, so switching back on is one checkbox rather
than four credentials retyped. That is the whole point — before the flag, "off" was expressible only
by deleting a working block.

It governs the **common room only**:

- **Off:** no target for the supervisor, no peer discovery assembled, no advertisement published,
  and no common-room token minted by either mint. The web app constructs no `Room` and mints no
  token; the host directory reports the LiveKit source as `idle`, never `error`.
- **Still on either way:** authentication. A daemon signs session tokens with its own Ed25519 key,
  not with anything in the `livekit:` block, so a daemon with the common room off — or with no
  `livekit:` block at all — still authenticates its own gated RPCs, including `DaemonConfigService`,
  the one an operator switches it back on from. Per-session rooms, screen sharing and the
  `#/livekit` rooms panel read the same block for their own purposes and are not governed by the
  flag.

## Runtime reconfiguration

Only the LiveKit block applies live. A supervisor owns the running common-room connection and, when
the switch, the URL or the room name changes, **tears the current one down before bringing the new
one up**. Saving the toggle therefore disconnects a live room, and re-enabling rejoins it — neither
needs a restart.

The failure mode is deliberate: a block missing any of the four fields it needs to be joinable never
becomes a target, so the daemon ends up **disconnected**, with a warning naming what was missing —
never silently still in the old room while the settings screen reports the new one. A daemon that was
*told* not to join is logged differently, at info rather than warning: an operator who switched
LiveKit off has not misconfigured anything, and the two must not read alike.

## Known limits

- **Comments are lost.** Writing re-serializes the config, so an operator's YAML comments do not
  survive a save. Field values do — verified by round-tripping `dev.daemon.yaml`, `dev.desktop.yaml`
  and `config.example.yaml`. Preserving comments needs a comment-aware YAML editor. The desktop's
  first-login enrolment writes the same file and has the same loss; a save and an enrolment are
  serialised against each other, so neither drops the other's `users:` row.
- **Peer discovery does not follow a runtime reconnect.** A daemon that *gains* a common room at
  runtime serves its roster there but does not discover the other daemons in it until restarted.
- **The UI edits the LiveKit block only.** Everything else is read-only and reported as
  restart-required.

## Related docs

- [Tddy desktop app (Tauri)](../desktop/tddy-desktop-tauri.md)
