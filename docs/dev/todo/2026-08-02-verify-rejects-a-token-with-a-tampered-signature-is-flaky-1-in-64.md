# 2026-08-02 — `verify_rejects_a_token_with_a_tampered_signature` is flaky ~1-in-64

**Category:** Future enhancement

`packages/tddy-github/src/session_token.rs` — the test asserts `InvalidSignature` but intermittently
gets `Malformed`. Diagnosed exactly:

```rust
fn with_tampered_signature(token: &str) -> String {   // :232
    chars[last] = if chars[last] == 'A' { 'B' } else { 'A' };
```

An HMAC-SHA256 tag is 32 bytes → 43 base64url characters, and 43 mod 4 == 3, so the final character
carries 2 significant bits plus 4 bits that **must be zero** for the encoding to be canonical. `'A'`
is 0 and always decodes; `'B'` is 1 and leaves a non-zero trailing bit, which a strict decoder
rejects. So whenever the untampered tag happens to end in `'A'` — about 1 in 64 runs — the helper
substitutes `'B'` and the token fails to *decode*, never reaching the signature check.

Deterministic per token, probabilistic across runs, because the tag depends on the expiry timestamp
baked into each freshly minted token. Fix: tamper a character that keeps the encoding canonical (flip
one in the middle of the tag), or assert on the tampered *bytes* rather than a re-encoded string.

Verified pre-existing: fails on this branch only inside a full run, passes 11/11 in isolation, and
`session_token`'s 10 tests pass at `master` (2851a1b3) — `tddy-github` is untouched by the
tddy-supervisor changeset.
