use crate::security::types::SecurityError;
use crate::security::vencrypt::username_and_password;
use crate::security::AES_BLOCK_LEN;
use crate::security::ARD_FIELD_LEN;
use crate::security::DH_MIN_ELEMENT;
use crate::security::MAX_ARD_KEY_LEN;
use aes::cipher::{generic_array::GenericArray, BlockEncrypt, KeyInit};
use md5::{Digest, Md5};
use num_bigint::BigUint;
use rustls::crypto::ring;
use tddy_screenshare::Credentials;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// What an Apple Remote Desktop server sends before the client answers.
#[derive(Debug)]
pub(super) struct ArdParameters {
    pub(super) generator: u16,
    key_len: usize,
    pub(super) prime: BigUint,
    pub(super) server_public: BigUint,
}

/// Apple Remote Desktop (type 30): read the Diffie-Hellman parameters and the server's public
/// key, answer with the username and password (two 64-byte null-terminated fields) encrypted
/// with AES-128-ECB under the MD5 of the shared secret, followed by the client's public key.
pub(super) async fn apple_remote_desktop<S>(
    stream: &mut S,
    credentials: &Credentials,
) -> Result<(), SecurityError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (username, password) = username_and_password(credentials)?;
    if username.len() >= ARD_FIELD_LEN || password.len() >= ARD_FIELD_LEN {
        return Err(SecurityError::Protocol(format!(
            "Apple Remote Desktop accepts a username and password of at most {} bytes",
            ARD_FIELD_LEN - 1
        )));
    }
    let parameters = read_ard_parameters(stream).await?;

    let mut private_key = vec![0u8; parameters.key_len];
    fill_random(&mut private_key)?;
    let private_key = BigUint::from_bytes_be(&private_key);
    let client_public = BigUint::from(parameters.generator).modpow(&private_key, &parameters.prime);
    let shared_secret = parameters
        .server_public
        .modpow(&private_key, &parameters.prime);
    let aes_key = derive_aes_key(&shared_secret, parameters.key_len);

    let mut message = encrypt_credentials_block(&aes_key, username, password)?.to_vec();
    message.extend_from_slice(&be_padded(&client_public, parameters.key_len));
    stream.write_all(&message).await?;
    stream.flush().await?;
    Ok(())
}

/// Read generator, key length, prime and the server's public key, refusing values that would
/// make the shared secret predictable: a generator or public value below 2, or a public value
/// of `prime - 1` (whose powers are only ever +1 or -1).
pub(super) async fn read_ard_parameters<S>(stream: &mut S) -> Result<ArdParameters, SecurityError>
where
    S: AsyncRead + Unpin,
{
    let generator = stream.read_u16().await?;
    let key_len = stream.read_u16().await? as usize;
    if key_len == 0 || key_len > MAX_ARD_KEY_LEN {
        return Err(SecurityError::Protocol(format!(
            "unusable Apple Remote Desktop key length {key_len}"
        )));
    }
    let mut prime = vec![0u8; key_len];
    stream.read_exact(&mut prime).await?;
    let mut server_public = vec![0u8; key_len];
    stream.read_exact(&mut server_public).await?;
    let prime = BigUint::from_bytes_be(&prime);
    let server_public = BigUint::from_bytes_be(&server_public);

    let min_element = BigUint::from(DH_MIN_ELEMENT);
    let usable = generator >= u16::from(DH_MIN_ELEMENT)
        && server_public >= min_element
        && &server_public + 1u8 < prime;
    if !usable {
        return Err(SecurityError::Protocol(
            "the Apple Remote Desktop key parameters are not usable".to_owned(),
        ));
    }
    Ok(ArdParameters {
        generator,
        key_len,
        prime,
        server_public,
    })
}

/// The AES-128 key: the MD5 of the shared secret written big-endian in `key_len` bytes.
pub(super) fn derive_aes_key(shared_secret: &BigUint, key_len: usize) -> [u8; AES_BLOCK_LEN] {
    Md5::digest(be_padded(shared_secret, key_len)).into()
}

/// The credentials block — username field, then password field, each [`ARD_FIELD_LEN`] bytes —
/// encrypted with AES-128-ECB. Each field is its text, a NUL terminator and then random bytes,
/// as macOS clients send it, so equal credentials do not encrypt to equal blocks. Both must be
/// shorter than [`ARD_FIELD_LEN`] (the caller checks).
pub(super) fn encrypt_credentials_block(
    aes_key: &[u8; AES_BLOCK_LEN],
    username: &[u8],
    password: &[u8],
) -> Result<[u8; 2 * ARD_FIELD_LEN], SecurityError> {
    let mut block = [0u8; 2 * ARD_FIELD_LEN];
    fill_random(&mut block)?;
    block[..username.len()].copy_from_slice(username);
    block[username.len()] = 0;
    block[ARD_FIELD_LEN..ARD_FIELD_LEN + password.len()].copy_from_slice(password);
    block[ARD_FIELD_LEN + password.len()] = 0;

    let cipher = aes::Aes128::new(GenericArray::from_slice(aes_key));
    for chunk in block.chunks_exact_mut(AES_BLOCK_LEN) {
        cipher.encrypt_block(GenericArray::from_mut_slice(chunk));
    }
    Ok(block)
}

pub(super) fn fill_random(buf: &mut [u8]) -> Result<(), SecurityError> {
    ring::default_provider()
        .secure_random
        .fill(buf)
        .map_err(|_| {
            SecurityError::Protocol("the system random number generator failed".to_owned())
        })
}

/// `value` as big-endian bytes, left-padded with zeros to `len`.
pub(super) fn be_padded(value: &BigUint, len: usize) -> Vec<u8> {
    let bytes = value.to_bytes_be();
    let mut padded = vec![0u8; len.saturating_sub(bytes.len())];
    padded.extend_from_slice(&bytes);
    padded
}
