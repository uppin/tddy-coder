# 2026-10-20 — VNC Authentication is answered by the bridge, and a refusal is typed

**Type:** Fix

`#host-desktops` 6/8 ([#658](https://github.com/uppin/tddy-coder/pull/658)). `security::negotiate` runs VNC Authentication itself: it answers the
16-byte challenge with DES under the bit-reversed password key (`des = "0.9"`, a direct dependency
approved 2026-10-20 and already in `Cargo.lock` through picky-krb), then reads the SecurityResult. It
hands vnc-rs the same RFB 3.3 None replay the VeNCrypt and Apple Remote Desktop paths use. So a wrong
password is `SecurityError::Refused { reason }` on RFB 3.3, 3.7 and 3.8, never vnc-rs's untyped error.
`vnc_client` tags `Refused` as `tddy_screenshare::AuthenticationRefused`. vnc-rs's password callback
now answers with an error, because it is never reached. Production lines: `security.rs` about 449,
`vnc_client.rs` 349. Test: `a_refused_vnc_password_is_reported_as_an_authentication_failure`.
