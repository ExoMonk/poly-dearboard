use aes_gcm::{
    aead::{Aead, OsRng},
    AeadCore, Aes256Gcm, KeyInit,
};
use hmac::Mac;

type HmacSha256 = hmac::Hmac<sha2::Sha256>;

/// Derives a per-user encryption key from the server master key and user address.
/// `server_key` must be 32 bytes.
pub fn derive_user_key(server_key: &[u8; 32], user_address: &str) -> [u8; 32] {
    let mut mac =
        <HmacSha256 as Mac>::new_from_slice(server_key).expect("HMAC accepts any key length");
    mac.update(user_address.as_bytes());
    mac.finalize().into_bytes().into()
}

/// Encrypts plaintext with AES-256-GCM using a fresh random nonce.
/// `aad` is additional authenticated data (user address) — binds ciphertext to the user.
/// Returns `(ciphertext, nonce)`.
pub fn encrypt_secret(
    key: &[u8; 32],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), String> {
    let cipher = Aes256Gcm::new(key.into());
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    let payload = aes_gcm::aead::Payload {
        msg: plaintext,
        aad,
    };

    let ciphertext = cipher
        .encrypt(&nonce, payload)
        .map_err(|e| format!("encryption failed: {e}"))?;

    Ok((ciphertext, nonce.to_vec()))
}

/// Decrypts ciphertext with AES-256-GCM.
/// `aad` must match the value used during encryption.
pub fn decrypt_secret(
    key: &[u8; 32],
    ciphertext: &[u8],
    nonce: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new(key.into());

    let nonce = aes_gcm::Nonce::from_slice(nonce);

    let payload = aes_gcm::aead::Payload {
        msg: ciphertext,
        aad,
    };

    cipher
        .decrypt(nonce, payload)
        .map_err(|e| format!("decryption failed: {e}"))
}
