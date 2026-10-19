# 2026-10-20 — An empty TLS public key is accepted silently

**Category:** Deferred from `host-desktops-rdp-nla` (#654)
**Source:** `/validate-prod-ready` on #654, deferral approved by the developer.

`packages/tddy-rdp/src/rdp_client.rs`: `ironrdp_tls::extract_tls_server_public_key(..).unwrap_or(&[])`
falls back to an empty key. The fallback predates #654. With CredSSP on, the client proves it reached the
endpoint it authenticates to by echoing the server's TLS public key, so a certificate that yields no key
surfaces as a confusing NLA authentication failure rather than a clear error.

**What would close it:** refuse with an error naming the missing key when the server negotiated NLA. Leave
the TLS-only path alone unless the developer agrees, since that is a behaviour change.

**Why deferred:** it changes behaviour on a path #654 did not set out to change, and removing a fallback
needs the developer's consent (CLAUDE.md).
