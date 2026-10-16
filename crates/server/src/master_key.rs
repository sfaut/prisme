//! Master key for encrypting connection secrets.
//!
//! The key is 32 random bytes (AES-256). It is generated on first launch and
//! stored in base64 in the `prisme.key` file of the data folder. Whoever copies
//! this file along with the internal database reads the saved passwords: the
//! data folder must stay readable by the user only. Losing this file requires
//! typing the connection passwords again, without touching the rest. See
//! `docs/V1.md`, section 2.8.

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;

use aes_gcm::Aes256Gcm;
use aes_gcm::aead::{Generate, Key, KeyInit};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use secrecy::{ExposeSecret, SecretSlice, SecretString};

/// Name of the key file in the data folder.
pub const FILE_NAME: &str = "prisme.key";

/// Length of the key in bytes (AES-256).
const KEY_LENGTH: usize = 32;

/// Errors reading or creating the master key.
#[derive(Debug, thiserror::Error)]
pub enum MasterKeyError {
    /// The key file is unreadable or cannot be created.
    #[error("key file {path}: {source}")]
    Io { path: String, source: io::Error },

    /// The content of the file is not valid base64.
    #[error("key file {path}: unreadable content (base64 expected)")]
    InvalidEncoding { path: String },

    /// The decoded content does not have the length of an AES-256 key.
    #[error("key file {path}: {length} bytes instead of {KEY_LENGTH}")]
    InvalidLength { path: String, length: usize },
}

/// Master key in memory, wiped when dropped and never shown in a log.
pub struct MasterKey(SecretSlice<u8>);

impl MasterKey {
    /// Builds the AES-256-GCM cipher associated with the key.
    pub fn cipher(&self) -> Aes256Gcm {
        // The length is checked when MasterKey is built: failure is impossible.
        Aes256Gcm::new_from_slice(self.0.expose_secret()).expect("a 32-byte key")
    }

    /// Generates a new random key, without saving it.
    pub fn generate() -> Self {
        let key = Key::<Aes256Gcm>::generate();
        Self(SecretSlice::from(key.to_vec()))
    }
}

/// Reads the key from the file `path`, or generates it and saves it there if the file does not exist.
///
/// An existing but invalid file produces an error: it is never overwritten,
/// as the secrets already encrypted would become unreadable.
pub fn load_or_create(path: &Path) -> Result<MasterKey, MasterKeyError> {
    match std::fs::read_to_string(path) {
        Ok(text) => decode(path, SecretString::from(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => create(path),
        Err(source) => Err(io_error(path, source)),
    }
}

/// Decodes the base64 content of a key file.
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

/// Generates a key and writes it to a new file.
fn create(path: &Path) -> Result<MasterKey, MasterKeyError> {
    let key = MasterKey::generate();
    let text = SecretString::from(BASE64.encode(key.0.expose_secret()));

    let mut options = OpenOptions::new();
    // create_new: fails if the file appeared in the meantime, so as never to overwrite a key.
    options.write(true).create_new(true);
    // On Windows, %APPDATA% is already readable by the user only.
    // On Unix, the file is explicitly restricted to its owner.
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);

    let mut file = options
        .open(path)
        .map_err(|source| io_error(path, source))?;
    file.write_all(text.expose_secret().as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|source| io_error(path, source))?;

    tracing::info!("master key created: {}", path.display());
    Ok(key)
}

/// Builds an I/O error tied to the key file.
fn io_error(path: &Path, source: io::Error) -> MasterKeyError {
    MasterKeyError::Io {
        path: display(path),
        source,
    }
}

/// Displays a path for an error message.
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
        std::fs::write(&path, "not base64!").unwrap();

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
