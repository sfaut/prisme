//! Clef maîtresse du chiffrement des secrets de connexion.
//!
//! La clef fait 32 octets aléatoires (AES-256). Elle est générée au premier
//! lancement et stockée en base64 dans le fichier `prisme.key` du dossier de
//! données. Quiconque copie ce fichier avec la base interne lit les mots de
//! passe enregistrés : le dossier de données doit rester lisible par le seul
//! utilisateur. Perdre ce fichier oblige à ressaisir les mots de passe des
//! connexions, sans toucher au reste. Voir `docs/V1.md`, section 2.8.

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;

use aes_gcm::Aes256Gcm;
use aes_gcm::aead::{Generate, Key, KeyInit};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use secrecy::{ExposeSecret, SecretSlice, SecretString};

/// Nom du fichier de clef dans le dossier de données.
pub const FILE_NAME: &str = "prisme.key";

/// Longueur de la clef en octets (AES-256).
const KEY_LENGTH: usize = 32;

/// Erreurs de lecture ou de création de la clef maîtresse.
#[derive(Debug, thiserror::Error)]
pub enum MasterKeyError {
    /// Le fichier de clef est illisible ou impossible à créer.
    #[error("fichier de clef {path} : {source}")]
    Io { path: String, source: io::Error },

    /// Le contenu du fichier n'est pas du base64 valide.
    #[error("fichier de clef {path} : contenu illisible (base64 attendu)")]
    InvalidEncoding { path: String },

    /// Le contenu décodé n'a pas la longueur d'une clef AES-256.
    #[error("fichier de clef {path} : {length} octets au lieu de {KEY_LENGTH}")]
    InvalidLength { path: String, length: usize },
}

/// Clef maîtresse en mémoire, effacée à sa destruction et jamais affichée dans un journal.
pub struct MasterKey(SecretSlice<u8>);

impl MasterKey {
    /// Construit le chiffreur AES-256-GCM associé à la clef.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "utilisé par secrets, incrément suivant")
    )]
    pub fn cipher(&self) -> Aes256Gcm {
        // La longueur est vérifiée à la construction de MasterKey : l'échec est impossible.
        Aes256Gcm::new_from_slice(self.0.expose_secret()).expect("clef de 32 octets")
    }

    /// Génère une nouvelle clef aléatoire, sans l'enregistrer.
    pub fn generate() -> Self {
        let key = Key::<Aes256Gcm>::generate();
        Self(SecretSlice::from(key.to_vec()))
    }
}

/// Lit la clef du fichier `path`, ou la génère et l'y enregistre si le fichier n'existe pas.
///
/// Un fichier existant mais invalide produit une erreur : il n'est jamais
/// écrasé, car les secrets déjà chiffrés deviendraient illisibles.
pub fn load_or_create(path: &Path) -> Result<MasterKey, MasterKeyError> {
    match std::fs::read_to_string(path) {
        Ok(text) => decode(path, SecretString::from(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => create(path),
        Err(source) => Err(io_error(path, source)),
    }
}

/// Décode le contenu base64 d'un fichier de clef.
fn decode(path: &Path, text: SecretString) -> Result<MasterKey, MasterKeyError> {
    let bytes = BASE64.decode(text.expose_secret().trim()).map_err(|_| {
        MasterKeyError::InvalidEncoding {
            path: display(path),
        }
    })?;
    let bytes = SecretSlice::from(bytes);

    let length = bytes.expose_secret().len();
    if length != KEY_LENGTH {
        return Err(MasterKeyError::InvalidLength {
            path: display(path),
            length,
        });
    }
    Ok(MasterKey(bytes))
}

/// Génère une clef et l'écrit dans un nouveau fichier.
fn create(path: &Path) -> Result<MasterKey, MasterKeyError> {
    let key = MasterKey::generate();
    let text = SecretString::from(BASE64.encode(key.0.expose_secret()));

    let mut options = OpenOptions::new();
    // create_new : échoue si le fichier est apparu entre-temps, pour ne jamais écraser une clef.
    options.write(true).create_new(true);
    // Sous Windows, %APPDATA% n'est déjà lisible que par l'utilisateur.
    // Sous Unix, on restreint explicitement le fichier à son propriétaire.
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);

    let mut file = options
        .open(path)
        .map_err(|source| io_error(path, source))?;
    file.write_all(text.expose_secret().as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|source| io_error(path, source))?;

    tracing::info!("clef maîtresse créée : {}", path.display());
    Ok(key)
}

/// Construit une erreur d'entrée-sortie rattachée au fichier de clef.
fn io_error(path: &Path, source: io::Error) -> MasterKeyError {
    MasterKeyError::Io {
        path: display(path),
        source,
    }
}

/// Affiche un chemin pour un message d'erreur.
fn display(path: &Path) -> String {
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_then_reloads_the_same_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);

        let created = load_or_create(&path).unwrap();
        let reloaded = load_or_create(&path).unwrap();

        assert_eq!(created.0.expose_secret(), reloaded.0.expose_secret());
        assert_eq!(std::fs::read_to_string(&path).unwrap().len(), 44);
    }

    #[test]
    fn rejects_invalid_base64() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        std::fs::write(&path, "pas du base64 !").unwrap();

        assert!(matches!(
            load_or_create(&path),
            Err(MasterKeyError::InvalidEncoding { .. })
        ));
    }

    #[test]
    fn rejects_wrong_length_and_keeps_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let short = BASE64.encode([0u8; 16]);
        std::fs::write(&path, &short).unwrap();

        assert!(matches!(
            load_or_create(&path),
            Err(MasterKeyError::InvalidLength { length: 16, .. })
        ));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), short);
    }
}
