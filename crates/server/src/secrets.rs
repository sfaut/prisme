//! Chiffrement des secrets de connexion (mots de passe, plus tard clefs TLS ou SSH).
//!
//! Algorithme : AES-256-GCM, avec la clef maîtresse. Un secret chiffré est un
//! bloc d'octets, stocké tel quel dans une colonne `BLOB` de la base interne :
//!
//! ```text
//! [ nonce : 12 octets ][ texte chiffré : longueur du secret ][ étiquette : 16 octets ]
//! ```
//!
//! Le nonce est tiré au hasard à chaque chiffrement : deux chiffrements du même
//! secret donnent deux blocs différents. L'étiquette détecte toute altération.

use aes_gcm::aead::{Aead, Generate};
use aes_gcm::{Nonce, aead};
use secrecy::{ExposeSecret, SecretString};

use crate::master_key::MasterKey;

/// Longueur du nonce en octets.
const NONCE_LENGTH: usize = 12;

/// Longueur de l'étiquette d'authentification en octets.
const TAG_LENGTH: usize = 16;

/// Erreurs de chiffrement ou de déchiffrement d'un secret.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// Le chiffrement a échoué (secret de taille démesurée).
    #[error("chiffrement du secret impossible")]
    Encrypt,

    /// Le bloc est trop court pour contenir un nonce et une étiquette.
    #[error("secret chiffré tronqué ({0} octets)")]
    Truncated(usize),

    /// La clef ne correspond pas, ou le bloc a été altéré.
    #[error("déchiffrement impossible : clef maîtresse différente ou secret altéré")]
    Decrypt,

    /// Le secret déchiffré n'est pas du texte UTF-8.
    #[error("secret déchiffré invalide (UTF-8 attendu)")]
    InvalidUtf8,
}

/// Chiffre un secret avec la clef maîtresse et renvoie le bloc à stocker.
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

/// Déchiffre un bloc produit par [`encrypt`] avec la même clef maîtresse.
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
        let block = encrypt(&key, &secret("mot de passe é€")).unwrap();

        assert_eq!(
            block.len(),
            NONCE_LENGTH + "mot de passe é€".len() + TAG_LENGTH
        );
        assert_eq!(
            decrypt(&key, &block).unwrap().expose_secret(),
            "mot de passe é€"
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
