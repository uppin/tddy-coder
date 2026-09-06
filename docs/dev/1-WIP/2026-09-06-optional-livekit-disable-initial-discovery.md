# Initial discovery: an explicit LiveKit disable

**Stack:** `optional-livekit` — node 8 of 8. **Date:** 2026-09-06.

## The gap

Nodes 1–7 made LiveKit *optional* in the sense of "the app works when it is not configured". There is
no way to say **"it is configured, and I want it off."** Turning it off today means deleting a working
`livekit:` block and putting it back by hand later.

## Where "is LiveKit on?" is decided today

There is one canonical decision — `CommonRoomTarget::from_livekit`
(`packages/tddy-daemon/src/common_room_supervisor.rs:54`), which every *join* path funnels through —
and **seven other independent re-derivations**, each with a slightly different field set:

| Site | Requires |
|---|---|
| `common_room_supervisor.rs:54` `CommonRoomTarget::from_livekit` | url, api_key, api_secret, common_room |
| `livekit_peer_discovery.rs:640` `livekit_common_room_connect_strings` | same four, error-shaped |
| `auth.rs:272-292` `MintLiveKitToken` | same four, inline |
| `spawner.rs:1403` `livekit_common_room_is_set` | common_room only |
| `spawner.rs:1437` `livekit_creds_from_config` | url, api_key, api_secret |
| `livekit_rooms_stream.rs:106` `configured_roster` | url, api_key, api_secret |
| `session_room.rs:1830-1842` | url, api_key, api_secret |
| `screen_sharing_service.rs:232` | `livekit.is_some()` + per-field `require_field!` |

There is no `DaemonConfig::livekit_enabled()`. The only helper on the config is
`common_room_set_metadata_attempt_budget` (`config.rs:1113`), a timeout.

**A flag added naively becomes an eighth thing to forget.** It needs one predicate the others delegate
to, or the feature ships half-applied.

## The trap that shapes the whole design

**`packages/tddy-daemon/src/auth.rs:63-67` uses `livekit.api_secret` as the signing secret for
*session tokens*:**

```rust
let signing_secret = config.livekit.as_ref().and_then(|lk| lk.api_secret.clone());
```

With no signer, `user_resolver` returns `None` for every token (`auth.rs:124-133`) and **every gated
daemon RPC refuses — including `DaemonConfigService`.** An operator who disabled LiveKit that way
could not re-enable it from the UI. The same secret is read again at `runtime.rs:741-745` for the
local Unix socket.

So **"disabled" must mean: do not join, do not advertise, do not mint room tokens.** It must *not*
mean "treat the block as absent". This is the single most important constraint on the node.

## Other things that read the block and must keep working

`screen_sharing_service.rs:232`, `session_room.rs:1830`, `livekit_rooms_stream.rs:97` (the `#/livekit`
rooms panel) and `spawner.rs:1437` (per-session rooms) all read LiveKit config for purposes other
than the common room. Nothing panics — every site is `Option`-shaped — but `spawner.rs:1403`
`livekit_common_room_is_set` silently changes the **identity scheme** of spawned coders, so a disable
that leaves `common_room` set but unjoined would still produce common-room-style identities.

## Where the flag has to be plumbed

**Config.** `LiveKitConfig` (`config.rs:911-943`) carries `#[serde(deny_unknown_fields)]`, so the Rust
field and the YAML key must land together — a downgraded daemon rejects a config the newer UI wrote.
`Default` is hand-written (`:932`), so the new field needs an explicit entry there too.

**Proto** (`packages/tddy-service/proto/daemon_config.proto`, no `reserved` ranges anywhere):

| Message | Used | Next free |
|---|---|---|
| `LiveKitSettings` (`:76-89`) | 1–6 | **7** |
| `GetClientConfigResponse` (`:52-61`) | 1–7 | **8** |

**The reconnect gap.** `daemon_settings.rs:119` `common_room_changed` keys the reconnect decision on
`(url, common_room)` only. A new flag not added here means **the toggle saves and nothing
disconnects** — the test `keeps_the_common_room_connected_when_an_unrelated_field_changes`
(`tests/daemon_config_service.rs:272`) is exactly the trap.

**The merge gap.** `daemon_settings.rs:81` `merged_livekit` does `stored.cloned().unwrap_or_default()`,
so an *unrendered* field survives an update automatically — but a **rendered** one must be assigned
explicitly or the toggle never persists.

**`ClientConfig`** is built in three places that must agree: `packages/tddy-daemon/src/server.rs:46`
(fed by `main.rs:111-136`), `packages/tddy-coder/src/run.rs:1198`, and the proto mirror
`daemon_config_service.rs:193-216`. Struct at `packages/tddy-coder/src/web_server.rs:20-45`.

**Web.** `clientConfig.ts` needs four edits (interface `:24`, JSON interface `:35`, `fromJson` `:45`,
RPC branch `:80`) — there is no shared mapping table. The natural client-side short-circuit is
`useCommonRoom.ts:39-48`, which already returns before `generateToken` and before `new Room()`.
`liveKitSource.ts:79-82` inherits `idle` straight from that guard, so an explicitly-disabled source
reports `idle` for free — which is the behaviour node 2 already documents as "a choice, not a fault".

**Settings form.** `settingsForm.ts:19-29`: every field is `string` except the display-only
`livekitApiSecretSet`. `DaemonSettingsScreen.tsx:49` types its `edit` helper as
`(field, value: string)`, so a checkbox needs that widened or a sibling `toggle`. **There is no
existing checkbox in this form and no dirty tracking at all** — Save is disabled only while saving
(`:142`). Checkbox patterns to copy live in `CreateSessionPane.tsx:708` and
`AssistantToolPicker.tsx:55`. Test-id convention is `daemon-settings-<kebab-field>`, registered in
`cypress/support/testIds.ts:729-739`.

**Env overrides** are in `runtime.rs:323-374` `apply_env_overrides` (not `main.rs`); the LiveKit arms
never *create* a block. The sibling `apply_telegram_env_overrides` (`config.rs:1135`) does create one
and has a bool parser at `config.rs:1329-1338` worth copying if an override is wanted.

## Tests to mirror

- `common_room_supervisor.rs:705` `names_no_room_to_join_when_the_livekit_block_is_incomplete` — the
  table this node extends with "…nor when it is disabled"; `:647` for the startup case.
- `daemon_settings.rs:145-481` — pure rules, with `SettingsBuilder` (`:181`) where a
  `with_livekit_disabled` belongs.
- `tests/daemon_config_service.rs:249` / `:272` — the reconnect pair.
- `settingsForm.test.ts:120` — "carries every field an update replaces, not only the edited one".
- `HostDirectoryAcceptance.cy.tsx:173` — "calls an unconfigured common room idle, never an error", to
  mirror for *explicitly disabled*.
- `DaemonSettingsAcceptance.cy.tsx` + `cypress/support/drivers/daemonSettingsDriver.tsx`.

## Docs that become wrong

`docs/ft/daemon/daemon-settings.md:43-50`, `docs/ft/daemon/livekit-peer-discovery.md:8,:14`
("discovery is on iff `common_room` + credentials are set" — no longer true),
`packages/tddy-web/docs/host-directory.md:43-52`, and the example configs operators copy
(`dev.desktop.yaml:57`, `config.example.yaml`, `dev.daemon.yaml`).
