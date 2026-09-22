//! HKDF-SHA256 (RFC 5869), and the hex encoding the file format uses.
//!
//! Hand-rolled over `hmac` + `sha2` rather than taken from `hkdf`, which would be tidier and needs
//! CLAUDE.md § ASK approval before it is added. Only a single 32-byte output block is ever needed
//! here, so Expand is one HMAC invocation: `T(1) = HMAC(PRK, info || 0x01)`.

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::secret::{wipe, SecretBytes};

type HmacSha256 = Hmac<Sha256>;

/// `HMAC-SHA256(key, parts[0] || parts[1] || …)`, as key material.
fn hmac_sha256(key: &[u8], parts: &[&[u8]]) -> SecretBytes {
    // HMAC accepts a key of any length, so construction cannot fail.
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC-SHA256 takes a key of any length");
    for part in parts {
        mac.update(part);
    }
    let mut digest = mac.finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    wipe(&mut digest);
    let key = SecretBytes::new(out);
    wipe(&mut out);
    key
}

/// HKDF-Extract then HKDF-Expand to 32 bytes.
pub(crate) fn hkdf_sha256(salt: &[u8], ikm: &[u8], info: &[u8]) -> SecretBytes {
    let prk = hmac_sha256(salt, &[ikm]);
    hkdf_expand(&prk, info)
}

/// HKDF-Expand to 32 bytes from key material that is already uniformly random — the data key,
/// when a purpose-specific subkey is derived from it.
pub(crate) fn hkdf_expand(prk: &SecretBytes, info: &[u8]) -> SecretBytes {
    hmac_sha256(prk.expose(), &[info, &[0x01]])
}

/// A keyed, non-reversible name for `parts` — how a record is addressed without writing its
/// provider and account to disk.
pub(crate) fn keyed_name(key: &SecretBytes, parts: &[&[u8]]) -> String {
    to_hex(hmac_sha256(key.expose(), parts).expose())
}

pub(crate) fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[usize::from(byte >> 4)] as char);
        out.push(DIGITS[usize::from(byte & 0x0f)] as char);
    }
    out
}

/// `None` for anything that is not an even-length run of hex digits.
pub(crate) fn from_hex(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    hex.as_bytes()
        .chunks(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16)?;
            let low = (pair[1] as char).to_digit(16)?;
            u8::try_from(high * 16 + low).ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 5869 Appendix A.1, truncated to the 32 bytes this crate derives.
    #[test]
    fn derives_the_rfc_5869_test_vector() {
        let ikm = [0x0b_u8; 22];
        let salt = from_hex("000102030405060708090a0b0c").unwrap();
        let info = from_hex("f0f1f2f3f4f5f6f7f8f9").unwrap();

        let okm = hkdf_sha256(&salt, &ikm, &info);

        assert_eq!(
            to_hex(okm.expose()),
            "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf"
        );
    }

    #[test]
    fn hex_round_trips_and_rejects_what_is_not_hex() {
        assert_eq!(
            (
                from_hex(&to_hex(&[0x00, 0xab, 0xff])),
                from_hex("abc"),
                from_hex("zz")
            ),
            (Some(vec![0x00, 0xab, 0xff]), None, None)
        );
    }
}
