# 2026-09-07 — Host-scoped desktop targets, and starting a bridge for one

**Type:** Feature

Top node of the `#hosts-screen` stack ([#460](https://github.com/uppin/tddy-coder/pull/460)). See
the cross-package entry for the whole change:
[docs/dev/changesets/2026-09-07-host-desktop-connect.md](../../../../docs/dev/changesets/2026-09-07-host-desktop-connect.md).

`host_desktop_targets.rs`: `HostDesktopTarget`, the `HostDesktopTargetStore` trait and a
`FileHostDesktopTargetStore` keeping `{ hosts: { <daemon_instance_id>: [target] } }` in
`host-desktop-targets.json` under `host_registry_dir(tddy_data_dir)` — beside the known-hosts file
and the host keypair, because all three are scoped to a machine. A `BTreeMap` so the file is ordered
the same way between writes; a named wrapper object so a later field does not make written files
unreadable; `protocol` holding `screen_sharing.proto`'s `Protocol` discriminants rather than a
second enumeration of them. Published with plain `tddy_core::atomic_file` — the store holds no
credential, so the exclusion that keeps the daemon's secret files on a hand-rolled writer does not
apply — under a mutex held across each read-modify-write, since two concurrent attaches would
otherwise drop one another's target. `list` is fallible: a file that exists and does not parse is an
error, never an empty set, or the callers that go on to write would replace a damaged file with one
holding a single target.

`screen_sharing_service.rs` gains the host-scoped half of the surface: `list_host_targets`,
`add_host_target`, `start_host_stream`, `stop_host_stream`. All four go through `require_user` on
the caller's session token and `require_host_scope`; a daemon built without host scope answers
`FAILED_PRECONDITION` rather than a plausible empty result. `HostScope` carries the target store,
the host keypair and the host prompt registry, and `runtime.rs` builds **one** prompt registry
shared with `ConnectionService` — a prompt raised on one registry and answered on another is a
question nobody can answer.

`start_host_stream` resolves the target, the room (`livekit.common_room`; a host has no session
metadata to take one from, and a daemon with no LiveKit configuration is refused rather than handed
coordinates nothing can join — deliberately not gated on `livekit.enabled`, which governs whether
this daemon joins), the identity `screenshare-host-{instance_id}-{target_id}`, and the whole bridge
preparation **before** anyone is asked for a secret. It then raises a `PromptKind::DesktopPassword`
prompt stamped with the operator resolved from the session token, waits exactly as long as the
prompt is answerable, decrypts the answer with the host keypair, hands the plaintext to the bridge
on stdin and drops it. Nothing persists it and no argv carries it. An unanswered prompt is
`DeadlineExceeded` with no bridge spawned. Unlike the deliberately silent session-scoped spawn,
whose callers depend on getting coordinates back regardless, a host-scoped spawn failure is
reported; a reopen terminates the bridge the previous open left rather than orphaning its pid.
`answer_before_expiry` lives in `host_prompts.rs` beside the registry, since two callers need "wait
exactly as long as this prompt is answerable".

`stop_host_stream` checks host scope too: `ok: true` from a daemon that could not have been holding
a bridge open would tell the browser to tear down an overlay over a process still running.

Module [host-registry.md § Host-scoped desktop targets](../host-registry.md#host-scoped-desktop-targets);
the tooling probe that reports what is connectable, [host-tooling-probe.md](../host-tooling-probe.md).
