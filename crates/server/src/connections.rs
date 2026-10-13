//! Connexions enregistrées vers les bases cibles.
//!
//! Le mot de passe est chiffré avec la clef maîtresse avant d'être stocké, le
//! reste est en clair (voir `docs/V1.md`, section 2.8). Chaque connexion
//! appartient à un utilisateur : toutes les fonctions filtrent sur son
//! identifiant. Fonctions synchrones (`rusqlite`), appelées depuis des tâches
//! bloquantes.

use prisme_connectors::mysql::MysqlTarget;
use rusqlite::{Connection, OptionalExtension, ffi, params};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

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
#[derive(Deserialize, TS)]
#[ts(export)]
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
    #[ts(type = "string")]
    pub password: SecretString,
    /// Base sélectionnée à la connexion. Absente ou vide : aucune.
    #[serde(default)]
    #[ts(optional, as = "Option<String>")]
    pub database_name: String,
}

/// Paramètres saisis à tester, avant leur enregistrement.
#[derive(Deserialize, TS)]
#[ts(export)]
pub struct ProbeRequest {
    /// Connexion telle que saisie ; seul le nom n'est pas utilisé.
    pub connection: NewConnection,
    /// Connexion enregistrée en cours de modification : si le mot de passe
    /// saisi est vide, le sien est repris. Absente pour une création.
    #[serde(default)]
    #[ts(optional)]
    pub saved_id: Option<i64>,
}

/// Connexion enregistrée, sans son mot de passe.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
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
    result.map_err(|error| name_error(error, &new.name))?;
    Ok(db.last_insert_rowid())
}

/// Modifie la connexion `id` de l'utilisateur `user_id`. Un mot de passe vide
/// garde le mot de passe enregistré : l'interface ne le connaît pas, elle ne
/// peut pas le renvoyer.
///
/// Renvoie `false` si la connexion n'existe pas ou appartient à un autre utilisateur.
pub fn update(
    db: &Connection,
    key: &MasterKey,
    user_id: i64,
    id: i64,
    changed: &NewConnection,
) -> Result<bool, ConnectionsError> {
    changed.validate()?;
    let password_secret = if changed.password.expose_secret().is_empty() {
        None
    } else {
        Some(secrets::encrypt(key, &changed.password)?)
    };

    let result = db.execute(
        "UPDATE connections
         SET name = ?3, host = ?4, port = ?5, username = ?6,
             password_secret = COALESCE(?7, password_secret), database_name = ?8
         WHERE id = ?1 AND user_id = ?2",
        params![
            id,
            user_id,
            changed.name,
            changed.host,
            changed.port,
            changed.username,
            password_secret,
            changed.database_name,
        ],
    );
    let updated = result.map_err(|error| name_error(error, &changed.name))?;
    Ok(updated == 1)
}

/// Traduit la violation de l'unicité du nom en `DuplicateName`, laisse passer les autres erreurs.
fn name_error(error: rusqlite::Error, name: &str) -> ConnectionsError {
    match error {
        rusqlite::Error::SqliteFailure(failure, _)
            if failure.extended_code == ffi::SQLITE_CONSTRAINT_UNIQUE =>
        {
            ConnectionsError::DuplicateName(name.to_string())
        }
        error => error.into(),
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

/// Supprime la connexion `id` de l'utilisateur `user_id`.
///
/// Renvoie `false` si la connexion n'existe pas ou appartient à un autre utilisateur.
pub fn delete(db: &Connection, user_id: i64, id: i64) -> Result<bool, ConnectionsError> {
    let deleted = db.execute(
        "DELETE FROM connections WHERE id = ?1 AND user_id = ?2",
        params![id, user_id],
    )?;
    Ok(deleted == 1)
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

/// Paramètres de connexion d'un test de paramètres saisis. Un mot de passe
/// vide reprend celui de la connexion enregistrée `saved_id`, s'il y en a une.
///
/// Renvoie `None` si `saved_id` désigne une connexion absente ou appartenant
/// à un autre utilisateur.
pub fn probe_target(
    db: &Connection,
    key: &MasterKey,
    user_id: i64,
    request: ProbeRequest,
) -> Result<Option<MysqlTarget>, ConnectionsError> {
    let ProbeRequest {
        connection,
        saved_id,
    } = request;
    let password = match saved_id {
        Some(id) if connection.password.expose_secret().is_empty() => {
            match load_target(db, key, user_id, id)? {
                Some(saved) => saved.password,
                None => return Ok(None),
            }
        }
        _ => connection.password,
    };
    Ok(Some(MysqlTarget {
        host: connection.host,
        port: connection.port,
        username: connection.username,
        password,
        database_name: connection.database_name,
    }))
}

#[cfg(test)]
mod tests {
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
    fn delete_removes_only_the_users_connection() {
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let alice = store::ensure_user(&db, "alice@example.com").unwrap();
        let bob = store::ensure_user(&db, "bob@example.com").unwrap();
        let id = insert(&db, &key, alice, &new_connection("prod")).unwrap();

        assert!(!delete(&db, bob, id).unwrap());
        assert_eq!(list(&db, alice).unwrap().len(), 1);
        assert!(delete(&db, alice, id).unwrap());
        assert!(list(&db, alice).unwrap().is_empty());
        assert!(!delete(&db, alice, id).unwrap());
    }

    #[test]
    fn update_keeps_the_password_when_left_empty() {
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let alice = store::ensure_user(&db, "alice@example.com").unwrap();
        let bob = store::ensure_user(&db, "bob@example.com").unwrap();
        let id = insert(&db, &key, alice, &new_connection("prod")).unwrap();

        let mut changed = new_connection("recette");
        changed.password = SecretString::from("");
        assert!(update(&db, &key, alice, id, &changed).unwrap());
        assert_eq!(list(&db, alice).unwrap()[0].name, "recette");
        let target = load_target(&db, &key, alice, id).unwrap().unwrap();
        assert_eq!(target.password.expose_secret(), "secret");

        changed.password = SecretString::from("nouveau");
        assert!(update(&db, &key, alice, id, &changed).unwrap());
        let target = load_target(&db, &key, alice, id).unwrap().unwrap();
        assert_eq!(target.password.expose_secret(), "nouveau");

        assert!(!update(&db, &key, bob, id, &changed).unwrap());
    }

    #[test]
    fn update_rejects_a_name_already_taken() {
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();
        insert(&db, &key, user_id, &new_connection("prod")).unwrap();
        let id = insert(&db, &key, user_id, &new_connection("recette")).unwrap();

        assert!(matches!(
            update(&db, &key, user_id, id, &new_connection("prod")),
            Err(ConnectionsError::DuplicateName(_))
        ));
    }

    #[test]
    fn probe_target_takes_the_saved_password_when_left_empty() {
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let alice = store::ensure_user(&db, "alice@example.com").unwrap();
        let bob = store::ensure_user(&db, "bob@example.com").unwrap();
        let id = insert(&db, &key, alice, &new_connection("prod")).unwrap();
        let request = |password: &str, saved_id| {
            let mut connection = new_connection("");
            connection.password = SecretString::from(password);
            ProbeRequest {
                connection,
                saved_id,
            }
        };

        let target = probe_target(&db, &key, alice, request("", Some(id)));
        assert_eq!(target.unwrap().unwrap().password.expose_secret(), "secret");
        let target = probe_target(&db, &key, alice, request("autre", Some(id)));
        assert_eq!(target.unwrap().unwrap().password.expose_secret(), "autre");
        let target = probe_target(&db, &key, alice, request("", None));
        assert_eq!(target.unwrap().unwrap().password.expose_secret(), "");
        assert!(
            probe_target(&db, &key, bob, request("", Some(id)))
                .unwrap()
                .is_none()
        );
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
