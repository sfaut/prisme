//! Encryption of connection secrets (passwords, later TLS or SSH keys).
//!
//! Algorithm: AES-256-GCM, with the master key. An encrypted secret is a block
//! of bytes, stored as is in a `BLOB` column of the internal database:
//!
//! ```text
//! [ nonce: 12 bytes ][ ciphertext: length of the secret ][ tag: 16 bytes ]
//! ```
//!
//! The nonce is drawn at random on each encryption: two encryptions of the
//! same secret give two different blocks. The tag detects any alteration.

use aes_gcm::aead::{Aead, Generate};
use aes_gcm::{Nonce, aead};
use secrecy::{ExposeSecret, SecretString};

use crate::master_key::MasterKey;

/// Length of the nonce in bytes.
const NONCE_LENGTH: usize = 12;

/// Length of the authentication tag in bytes.
const TAG_LENGTH: usize = 16;

/// Errors encrypting or decrypting a secret.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// Encryption failed (secret of excessive size).
    #[error("cannot encrypt the secret")]
    Encrypt,

    /// The block is too short to contain a nonce and a tag.
    #[error("truncated encrypted secret ({0} bytes)")]
    Truncated(usize),

    /// The key does not match, or the block was altered.
    #[error("cannot decrypt: different master key or altered secret")]
    Decrypt,

    /// The decrypted secret is not UTF-8 text.
    #[error("invalid decrypted secret (UTF-8 expected)")]
    InvalidUtf8,
}

/// Encrypts a secret with the master key and returns the block to store.
pub fn encrypt(key: &MasterKey, secret: &SecretString) -> Result<Vec<u8>, SecretError> {
    let nonce = Nonce::generate();
    let ciphertext = key
        .cipher()
        .encrypt(&nonce, secret.expose_secret().as_bytes())
        .map_err(|_: aead::Error| SecretError::Encrypt)?;

    let mut block = Vec::with_capacity(NONCE_LENGTH + ciphertext.len());
    block.extend_from_slice(&nonce);
    block.extend_from_slice(&ciphertext);
    Ok(block)
}

/// Decrypts a block produced by [`encrypt`] with the same master key.
pub fn decrypt(key: &MasterKey, block: &[u8]) -> Result<SecretString, SecretError> {
    if block.len() < NONCE_LENGTH + TAG_LENGTH {
        return Err(SecretError::Truncated(block.len()));
    }
    let (nonce, ciphertext) = block.split_at(NONCE_LENGTH);
    let nonce = Nonce::try_from(nonce).map_err(|_| SecretError::Truncated(block.len()))?;

    let plaintext = key
        .cipher()
        .decrypt(&nonce, ciphertext)
        .map_err(|_: aead::Error| SecretError::Decrypt)?;
    let text = String::from_utf8(plaintext).map_err(|_| SecretError::InvalidUtf8)?;
    Ok(SecretString::from(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(text: &str) -> SecretString {
        SecretString::from(text)
    }

    #[test]
    fn round_trip() {
        let key = MasterKey::generate();
        let block = encrypt(&key, &secret("pass word é€")).unwrap();

        assert_eq!(
            block.len(),
            NONCE_LENGTH + "pass word é€".len() + TAG_LENGTH
        );
        assert_eq!(
            decrypt(&key, &block).unwrap().expose_secret(),
            "pass word é€"
        );
    }

    #[test]
    fn same_secret_gives_different_blocks() {
        let key = MasterKey::generate();
        assert_ne!(
            encrypt(&key, &secret("x")).unwrap(),
            encrypt(&key, &secret("x")).unwrap()
        );
    }

    #[test]
    fn wrong_key_is_rejected() {
        let block = encrypt(&MasterKey::generate(), &secret("x")).unwrap();
        assert!(matches!(
            decrypt(&MasterKey::generate(), &block),
            Err(SecretError::Decrypt)
        ));
    }

    #[test]
    fn altered_block_is_rejected() {
        let key = MasterKey::generate();
        let mut block = encrypt(&key, &secret("x")).unwrap();
        block[NONCE_LENGTH] ^= 1;
        assert!(matches!(decrypt(&key, &block), Err(SecretError::Decrypt)));
    }

    #[test]
    fn truncated_block_is_rejected() {
        let key = MasterKey::generate();
        assert!(matches!(
            decrypt(&key, &[0u8; 20]),
            Err(SecretError::Truncated(20))
        ));
    }
}
