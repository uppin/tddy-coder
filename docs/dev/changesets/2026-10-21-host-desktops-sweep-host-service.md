# 2026-10-21 — `#host-desktops` sweep: `host_tooling.rs` and `service.rs` split, desktop start paths shortened

**Type:** Refactor

`#host-desktops` 10/10 ([#693](https://github.com/uppin/tddy-coder/pull/693)), across `tddy-host-service`,
`tddy-screen-sharing`, `tddy-screenshare` and `tddy-rdp`; an independent sibling of #692 (the
`tddy-daemon-kernel` config split). No behaviour change; package entries: `2026-10-21-host-tooling-and-service-split`,
`2026-10-21-start-paths-shortened`, `2026-10-21-start-bridge-helper`, `2026-10-21-connect-helpers`.

**How it was cut.** Engine plans did every module cut and helper extraction. The engine refuses `extract_method`
inside an `#[async_trait]` impl (three probes reconfirmed it), so eleven trait-method bodies — `HostService`'s
nine, `ScreenSharingService::start_stream`, `ScreenSharingClient::connect` for `RdpClient` — were **moved by hand
with the developer's consent** into inherent `*_impl` methods behind one-line delegators, each proved
byte-identical by script, and the engine extracted from those. Five hand fixes followed engine applies, each a
recorded engine defect: a `pub(super)` on a private type a widened field names (E0446), a stray `*` before
`format!` (E0277), an unused copied generic (E0283 / clippy), a `let`-and-return (clippy), and `#[cfg(unix)]` lost
from re-pointed imports. One extraction was refused and not worked around (`finalize_connection` in `RdpClient`,
E0121). `restructure verify --against origin/master` accounted every statement but two (`tokens lost: none`):
an unused `mut` that `cargo fix` removed and one call whose `&String` arguments are now passed through a helper's
`&str` parameters — both shown equivalent by reading the diff.

**Measurements** (production lines; both measures agree): `host_tooling.rs` 806 ➜ 404, `service.rs` 972 ➜ 242,
`start_stream` 72 ➜ 41, `start_host_stream_impl` 76 ➜ 53, `start_bridge` 58 ➜ 31, `add_host_key` 86 ➜ 48,
`RdpClient::connect` 115 ➜ 72. `stream_host_stats` stays 96 (out of scope).

**Closed code issues** (deleted): `tddy-host-service` `oversized-file-host-tooling` (806 ➜ 404; 775 at detection
2026-10-15) and `oversized-file-service` (972 ➜ 242; 939 at detection 2026-09-24).

**Resolved backlog entries** (deleted): `2026-10-20-hosts-mount-service-rs-grew-while-over-budget`,
`2026-10-20-screen-sharing-start-functions-over-length`. **Narrowed:** `2026-10-20-rdp-connect-config-builder-extraction`
(the finalize step remains). **Kept:** `2026-10-20-screen-sharing-two-stand-in-bridges` (developer decision; no
engine operation reaches it). **Added:** six engine-defect and code-quality entries dated 2026-10-21
(`restructure-widened-field-leaves-its-private-type`, `restructure-extract-assist-writes-a-stray-deref-before-format`,
`restructure-extract-assist-copies-an-unused-generic`, `restructure-extract-assist-cannot-spell-an-opaque-parameter-type`,
`restructure-extract-leaves-a-let-and-return`, `restructure-move-drops-cfg-on-repointed-imports`) and
`desktop-start-helpers-parameter-count-and-nesting`; `2026-10-19-restructure-extract-method-refused-under-async-trait`
gained the three probes as evidence.

**Baseline:** 298 passed / 0 failed / 0 ignored before and after (`tddy-host-service` 199, `tddy-screen-sharing` 79,
`tddy-screenshare` 9, `tddy-rdp` 11), scoped `cargo test` per package; clippy `-D warnings` and `cargo fmt --check`
clean on all four. `tddy-screenshare` also carries `2026-10-21-bridge-ready-link-correction`.
