//! Connexions enregistrées vers les bases cibles.
//!
//! Le mot de passe est chiffré avec la clef maîtresse avant d'être stocké, le
//! reste est en clair (voir `docs/V1.md`, section 2.8). Chaque connexion
//! appartient à un utilisateur : toutes les fonctions filtrent sur son
//! identifiant. Fonctions synchrones (`rusqlite`), appelées depuis des tâches
//! bloquantes.

use prisme_connectors::mysql::MysqlTarget;
use rusqlite::{Connection, OptionalExtension, ffi, params};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};

use crate::master_key::MasterKey;
use crate::secrets::{self, SecretError};

/// Longueur maximale du nom d'une connexion, en caractères.
const NAME_MAX_LENGTH: usize = 100;

/// Longueur maximale d'un nom d'hôte (limite DNS), en caractères.
const HOST_MAX_LENGTH: usize = 253;

/// Longueur maximale d'un compte de connexion (limite MariaDB), en caractères.
const USERNAME_MAX_LENGTH: usize = 80;

/// Longueur maximale d'un nom de base (limite MySQL et MariaDB), en caractères.
const DATABASE_NAME_MAX_LENGTH: usize = 64;

/// Famille de moteurs des connexions MySQL et MariaDB.
const ENGINE_MYSQL: &str = "mysql";

/// Connexion à enregistrer, telle que saisie par l'utilisateur.
#[derive(Deserialize)]
pub struct NewConnection {
    /// Nom choisi par l'utilisateur, unique parmi ses connexions.
    pub name: String,
    /// Nom d'hôte ou adresse IP du serveur.
    pub host: String,
    /// Port TCP du serveur.
    pub port: u16,
    /// Compte de connexion.
    pub username: String,
    /// Mot de passe du compte, chiffré avant stockage.
    pub password: SecretString,
    /// Base sélectionnée à la connexion. Absente ou vide : aucune.
    #[serde(default)]
    pub database_name: String,
}

/// Connexion enregistrée, sans son mot de passe.
#[derive(Debug, Serialize)]
pub struct ConnectionSummary {
    /// Identifiant de la connexion.
    pub id: i64,
    /// Nom choisi par l'utilisateur.
    pub name: String,
    /// Famille de moteurs (`mysql` pour MySQL et MariaDB).
    pub engine: String,
    /// Nom d'hôte ou adresse IP du serveur.
    pub host: String,
    /// Port TCP du serveur.
    pub port: u16,
    /// Compte de connexion.
    pub username: String,
    /// Base sélectionnée à la connexion, chaîne vide si aucune.
    pub database_name: String,
    /// Date d'enregistrement, en UTC au format ISO 8601.
    pub created_at: String,
}

/// Erreurs de gestion des connexions enregistrées.
#[derive(Debug, thiserror::Error)]
pub enum ConnectionsError {
    /// Saisie invalide : le message dit quel champ et pourquoi.
    #[error("{0}")]
    Invalid(String),

    /// L'utilisateur a déjà une connexion de ce nom.
    #[error("une connexion nommée « {0} » existe déjà")]
    DuplicateName(String),

    /// Chiffrement ou déchiffrement du mot de passe en échec.
    #[error(transparent)]
    Secret(#[from] SecretError),

    /// Erreur de la base interne.
    #[error("base interne : {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl NewConnection {
    /// Vérifie la saisie, avec les mêmes limites que la table `connections`.
    pub fn validate(&self) -> Result<(), ConnectionsError> {
        check_length("name", &self.name, 1, NAME_MAX_LENGTH)?;
        check_length("host", &self.host, 1, HOST_MAX_LENGTH)?;
        check_length("username", &self.username, 1, USERNAME_MAX_LENGTH)?;
        check_length(
            "database_name",
            &self.database_name,
            0,
            DATABASE_NAME_MAX_LENGTH,
        )?;
        if self.port == 0 {
            return Err(ConnectionsError::Invalid(
                "port : doit être compris entre 1 et 65535".to_string(),
            ));
        }
        Ok(())
    }
}

/// Vérifie qu'un champ compte entre `min` et `max` caractères (et non octets, comme `length()` de SQLite).
fn check_length(field: &str, value: &str, min: usize, max: usize) -> Result<(), ConnectionsError> {
    let length = value.chars().count();
    if length < min || length > max {
        return Err(ConnectionsError::Invalid(format!(
            "{field} : entre {min} et {max} caractères attendus, {length} reçus"
        )));
    }
    Ok(())
}

/// Enregistre une connexion pour l'utilisateur `user_id` et renvoie son identifiant.
pub fn insert(
    db: &Connection,
    key: &MasterKey,
    user_id: i64,
    new: &NewConnection,
) -> Result<i64, ConnectionsError> {
    new.validate()?;
    let password_secret = secrets::encrypt(key, &new.password)?;

    let result = db.execute(
        "INSERT INTO connections (user_id, name, engine, host, port, username, password_secret, database_name)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            user_id,
            new.name,
            ENGINE_MYSQL,
            new.host,
            new.port,
            new.username,
            password_secret,
            new.database_name,
        ],
    );
    match result {
        Ok(_) => Ok(db.last_insert_rowid()),
        Err(rusqlite::Error::SqliteFailure(error, _))
            if error.extended_code == ffi::SQLITE_CONSTRAINT_UNIQUE =>
        {
            Err(ConnectionsError::DuplicateName(new.name.clone()))
        }
        Err(error) => Err(error.into()),
    }
}

/// Liste les connexions de l'utilisateur `user_id`, par nom, sans leur mot de passe.
pub fn list(db: &Connection, user_id: i64) -> Result<Vec<ConnectionSummary>, ConnectionsError> {
    let mut statement = db.prepare(
        "SELECT id, name, engine, host, port, username, database_name, created_at
         FROM connections WHERE user_id = ?1 ORDER BY name",
    )?;
    let rows = statement.query_map(params![user_id], |row| {
        Ok(ConnectionSummary {
            id: row.get(0)?,
            name: row.get(1)?,
            engine: row.get(2)?,
            host: row.get(3)?,
            port: row.get(4)?,
            username: row.get(5)?,
            database_name: row.get(6)?,
            created_at: row.get(7)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Charge les paramètres de connexion, mot de passe déchiffré, de la connexion `id`.
///
/// Renvoie `None` si la connexion n'existe pas ou appartient à un autre utilisateur.
pub fn load_target(
    db: &Connection,
    key: &MasterKey,
    user_id: i64,
    id: i64,
) -> Result<Option<MysqlTarget>, ConnectionsError> {
    let row = db
        .query_row(
            "SELECT host, port, username, password_secret, database_name
             FROM connections WHERE id = ?1 AND user_id = ?2",
            params![id, user_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u16>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()?;

    let Some((host, port, username, password_secret, database_name)) = row else {
        return Ok(None);
    };
    Ok(Some(MysqlTarget {
        host,
        port,
        username,
        password: secrets::decrypt(key, &password_secret)?,
        database_name,
    }))
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret;

    use super::*;
    use crate::store;

    fn new_connection(name: &str) -> NewConnection {
        NewConnection {
            name: name.to_string(),
            host: "localhost".to_string(),
            port: 3306,
            username: "root".to_string(),
            password: SecretString::from("secret"),
            database_name: String::new(),
        }
    }

    #[test]
    fn insert_list_and_load() {
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();

        let id = insert(&db, &key, user_id, &new_connection("prod")).unwrap();

        let summaries = list(&db, user_id).unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].name, "prod");
        assert_eq!(summaries[0].engine, "mysql");

        let target = load_target(&db, &key, user_id, id).unwrap().unwrap();
        assert_eq!(target.password.expose_secret(), "secret");
    }

    #[test]
    fn password_is_stored_encrypted() {
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();
        insert(&db, &key, user_id, &new_connection("prod")).unwrap();

        let stored: Vec<u8> = db
            .query_row("SELECT password_secret FROM connections", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(!stored.windows(6).any(|window| window == b"secret"));
    }

    #[test]
    fn duplicate_name_is_rejected() {
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();
        insert(&db, &key, user_id, &new_connection("prod")).unwrap();

        assert!(matches!(
            insert(&db, &key, user_id, &new_connection("prod")),
            Err(ConnectionsError::DuplicateName(_))
        ));
    }

    #[test]
    fn users_only_see_their_connections() {
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let alice = store::ensure_user(&db, "alice@example.com").unwrap();
        let bob = store::ensure_user(&db, "bob@example.com").unwrap();
        let id = insert(&db, &key, alice, &new_connection("prod")).unwrap();

        assert!(list(&db, bob).unwrap().is_empty());
        assert!(load_target(&db, &key, bob, id).unwrap().is_none());
        // Le même nom reste libre pour un autre utilisateur.
        assert!(insert(&db, &key, bob, &new_connection("prod")).is_ok());
    }

    #[test]
    fn validation_counts_characters() {
        let mut connection = new_connection(&"é".repeat(NAME_MAX_LENGTH));
        assert!(connection.validate().is_ok());

        connection.name.push('é');
        assert!(matches!(
            connection.validate(),
            Err(ConnectionsError::Invalid(_))
        ));
    }

    #[test]
    fn validation_rejects_empty_fields_and_port_zero() {
        let mut connection = new_connection("");
        assert!(connection.validate().is_err());

        connection = new_connection("prod");
        connection.port = 0;
        assert!(connection.validate().is_err());
    }
}
