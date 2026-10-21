# 2026-10-20 — An NLA refusal is tagged as a refused sign-in

**Type:** Fix

`#host-desktops` 6/8 ([#658](https://github.com/uppin/tddy-coder/pull/658)). `RdpClient::connect` wraps a CredSSP connector error in
`tddy_screenshare::AuthenticationRefused` (context still "connect_finalize: NLA authentication
failed"), so the bridge reports it as an authentication failure. Test:
`a_wrong_password_on_an_nla_server_is_tagged_as_a_refused_sign_in` in `tests/rdp_nla_integration.rs`.
