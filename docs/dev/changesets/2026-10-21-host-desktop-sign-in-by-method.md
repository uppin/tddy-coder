# 2026-10-21 — Host desktop sign-in shaped by its advertised methods, with Remember

**Type:** Feature

`#host-desktops` 8/8 ([#660](https://github.com/uppin/tddy-coder/pull/660)). This is the cross-package view; each package's own entry is in its
`docs/changesets/`.

- **Protos** (`tddy-service`). `ScreenSharingService.InspectHostDesktop` with
  `InspectHostDesktopResponse { offered, unsupported_vnc_types, remembered }`. `enum
  DesktopAuthMethod` and `message DesktopCredentialAnswer { method, username, domain, password,
  remember }`. `domain` on `ScreenSharingTarget` and `AddHostTargetRequest`.
  `HostPromptKind.DESKTOP_CREDENTIALS`, and `HostPromptEvent.offered_auth_methods`, which is sent
  empty.
- **Bridges.** An `--inspect` mode: VNC reads the RFB security and VeNCrypt subtype lists, and RDP
  reads the X.224 negotiation. The inspect line's types are in
  `tddy_daemon_extensions::desktop_inspect`, so `tddy-screen-sharing` reads them without depending
  on the LiveKit plugin crate `tddy-screenshare`.
- **Daemon** (`tddy-screen-sharing`). Inspection runs the bridge. A start uses a remembered
  credential from the operator's vault, or prompts `DESKTOP_CREDENTIALS`. It writes Remember only
  after the bridge has started, and marks a refused remembered credential refused. A locked vault
  asks.
- **Web** (`tddy-web`). `HostDesktopSignInDialog` (fields by method, method switcher, Remember),
  and `HostKeyContinuityNotices`, shared with `HostPassphraseDialog`. `HostDesktopOverlay`
  inspects first, answers a `NONE` desktop itself, and re-asks on `Unauthenticated`.
- **Deferred** (new `docs/dev/todo/` entries): the sign-in dialog pins the advertised fingerprint
  rather than deriving it (`2026-10-21-desktop-sign-in-pins-the-advertised-fingerprint`), and the
  unused prompt field (`2026-10-21-host-prompt-offered-auth-methods-is-never-sent`).
- **Removed.** `HostPassphraseDialog`'s `allowEmpty` and `wording`, and
  `PromptKind::DesktopPassword`. The proto value stays and is raised by nothing. The web test
  `says the answer may be left blank when the desktop has no password` is removed: a `NONE` desktop
  gets no form, which is pinned by `opens a desktop that has no password without asking the
  operator` and `opens a desktop that needs no sign in without asking`.
- **Code issues**: none of the touched files carries a record. Every touched non-test file stays
  under 500 production lines. `screen_sharing_service.rs` went from 450 to 482, and
  `start_host_stream_impl` from 70 to 76 lines; the remainder is
  `2026-10-20-screen-sharing-start-functions-over-length`.
