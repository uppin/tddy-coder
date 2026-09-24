# 2026-09-24 — the vault's cipher and HMAC key schedules are not wiped on drop

**Category:** Key lifetime — needs a dependency decision (CLAUDE.md § ASK)
**Source:** `#keyring` 3/9 `/validate-prod-ready` (#510); `TODO(keyring)` in
`packages/tddy-credentials/src/vault/crypto.rs` (`fn cipher`)
**Package:** `tddy-credentials`

## What happens

`tddy-credentials` wipes the key material it owns — `SecretBytes`, `SecretString` and the transient
plaintext buffers — with a volatile write and a compiler fence. The `ChaCha20Poly1305` and HMAC
instances it builds from a key hold their **own** copy of the expanded key schedule, and those are
dropped without being wiped, so a copy of a data key or KEK can outlive the handle in freed memory.

## Why it is not fixed in #510

Wiping them needs `chacha20poly1305`'s (and `hmac`'s) `zeroize` feature, which pulls in the `zeroize`
crate — a new external dependency, and CLAUDE.md § ASK requires the developer's approval before one
is taken. #510 took neither `zeroize` nor `hkdf`, by decision.

## What would close it

With approval, enable the `zeroize` features on `chacha20poly1305` and `hmac` (and, while there,
replace the hand-rolled wipe with `zeroize::Zeroizing`), then remove the `TODO(keyring)` in
`vault/crypto.rs` and the ⚠ in `packages/tddy-credentials/docs/credential-store.md` § Zeroization.
