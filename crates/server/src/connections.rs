//! Saved connections to the target databases.
//!
//! The password is encrypted with the master key before being stored, the
//! rest is in clear (see `docs/V1.md`, section 2.8). Each connection belongs
//! to a user: all the functions filter on their id. Synchronous functions
//! (`rusqlite`), called from blocking tasks.
//!
//! A connection can carry queries to run each time a connection to the
//! database opens (`SET NAMES utf8mb4; SET SESSION sql_mode = ...`): only
//! session settings are allowed there (see
//! `prisme_dialects::classify::session_setting_refusal`).
//!
//! It also sets the delay during which a query session stays open after its
//! page is closed (see `sessions.rs`), and remembers the server's flavor
//! (MySQL or MariaDB) seen on the last connection.

use prisme_connectors::mysql::{Flavor, MysqlTarget, ServerInfo};
use prisme_dialects::Dialect;
use prisme_dialects::classify::{SettingRefusal, session_setting_refusal};
use rusqlite::{Connection, OptionalExtension, ffi, params};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error_detail::{ErrorDetail, Field};
use crate::master_key::MasterKey;
use crate::secrets::{self, SecretError};

/// Maximum length of a connection's name, in characters.
const NAME_MAX_LENGTH: usize = 100;

/// Maximum length of a host name (DNS limit), in characters.
const HOST_MAX_LENGTH: usize = 253;

/// Maximum length of a connection account (MariaDB limit), in characters.
const USERNAME_MAX_LENGTH: usize = 80;

/// Maximum length of a database name (MySQL and MariaDB limit), in characters.
const DATABASE_NAME_MAX_LENGTH: usize = 64;

/// Maximum length of the on-connect queries, in characters.
const INIT_SQL_MAX_LENGTH: usize = 4000;

/// Maximum keep-alive delay of a detached session, in minutes (24 hours).
const SESSION_TIMEOUT_MAX_MINUTES: u32 = 1440;

/// Keep-alive delay of a detached session, in minutes, when the input gives none.
const SESSION_TIMEOUT_DEFAULT_MINUTES: u32 = 5;

/// Family of engines of the MySQL and MariaDB connections.
const ENGINE_MYSQL: &str = "mysql";

/// Connection to save, as typed by the user.
#[derive(Deserialize, TS)]
#[ts(export)]
pub struct NewConnection {
    /// Name chosen by the user, unique among their connections.
    pub name: String,
    /// Host name or IP address of the server.
    pub host: String,
    /// TCP port of the server.
    pub port: u16,
    /// Connection account.
    pub username: String,
    /// Password of the account, encrypted before storage.
    #[ts(type = "string")]
    pub password: SecretString,
    /// Database selected on connection. Absent or empty: none.
    #[serde(default)]
    #[ts(optional, as = "Option<String>")]
    pub database_name: String,
    /// Queries to run each time a connection opens, separated by ";":
    /// session settings only. Absent or empty: none.
    #[serde(default)]
    #[ts(optional, as = "Option<String>")]
    pub init_sql: String,
    /// Delay during which a session stays open after its page is closed, in
    /// minutes (0: closed at once). Absent: 5 minutes.
    #[serde(default = "default_session_timeout")]
    #[ts(optional, as = "Option<u32>")]
    pub session_timeout_minutes: u32,
    /// Does the navigator show the system databases (`information_schema`,
    /// `mysql`...)? Absent: yes.
    #[serde(default = "default_show_system_schemas")]
    #[ts(optional, as = "Option<bool>")]
    pub show_system_schemas: bool,
}

/// Default display of the system databases, for deserialising `NewConnection`.
fn default_show_system_schemas() -> bool {
    true
}

/// Default keep-alive delay, for deserialising `NewConnection`.
fn default_session_timeout() -> u32 {
    SESSION_TIMEOUT_DEFAULT_MINUTES
}

/// Typed settings to test, before saving them.
#[derive(Deserialize, TS)]
#[ts(export)]
pub struct ProbeRequest {
    /// Connection as typed; only the name is not used.
    pub connection: NewConnection,
    /// Saved connection being edited: if the typed password is empty, its own
    /// is used. Absent for a creation.
    #[serde(default)]
    #[ts(optional)]
    pub saved_id: Option<i64>,
}

/// Saved connection, without its password.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ConnectionSummary {
    /// Id of the connection.
    pub id: i64,
    /// Name chosen by the user.
    pub name: String,
    /// Family of engines (`mysql` for MySQL and MariaDB).
    pub engine: String,
    /// Host name or IP address of the server.
    pub host: String,
    /// TCP port of the server.
    pub port: u16,
    /// Connection account.
    pub username: String,
    /// Database selected on connection, empty string if none.
    pub database_name: String,
    /// Queries run each time a connection opens, empty string if none.
    pub init_sql: String,
    /// Keep-alive delay of a session after its page is closed, in minutes.
    pub session_timeout_minutes: u32,
    /// Does the navigator show the system databases?
    pub show_system_schemas: bool,
    /// Flavor of the server seen on the last successful connection, `null`
    /// if the connection never succeeded.
    pub server_flavor: Option<Flavor>,
    /// Version of the server seen on the last successful connection (raw
    /// result of `SELECT VERSION()`), `null` if the connection never succeeded.
    pub server_version: Option<String>,
    /// Date it was saved, in UTC, in ISO 8601 format.
    pub created_at: String,
}

/// Errors managing saved connections.
#[derive(Debug, thiserror::Error)]
pub enum ConnectionsError {
    /// Invalid input: the detail says which field and why.
    #[error("{0}")]
    Invalid(ErrorDetail),

    /// The user already has a connection with this name.
    #[error("a connection named \"{0}\" already exists")]
    DuplicateName(String),

    /// Encryption or decryption of the password failed.
    #[error(transparent)]
    Secret(#[from] SecretError),

    /// Error of the internal database.
    #[error("internal database: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl NewConnection {
    /// Checks the input, with the same limits as the `connections` table.
    pub fn validate(&self) -> Result<(), ConnectionsError> {
        check_length(Field::Name, &self.name, 1, NAME_MAX_LENGTH)?;
        check_length(Field::Host, &self.host, 1, HOST_MAX_LENGTH)?;
        check_length(Field::Username, &self.username, 1, USERNAME_MAX_LENGTH)?;
        check_length(
            Field::DatabaseName,
            &self.database_name,
            0,
            DATABASE_NAME_MAX_LENGTH,
        )?;
        if self.port == 0 {
            return Err(ConnectionsError::Invalid(ErrorDetail::FieldRange {
                field: Field::Port,
                min: 1,
                max: u16::MAX.into(),
            }));
        }
        check_length(Field::InitSql, &self.init_sql, 0, INIT_SQL_MAX_LENGTH)?;
        if self.session_timeout_minutes > SESSION_TIMEOUT_MAX_MINUTES {
            return Err(ConnectionsError::Invalid(ErrorDetail::FieldRange {
                field: Field::SessionTimeoutMinutes,
                min: 0,
                max: SESSION_TIMEOUT_MAX_MINUTES,
            }));
        }
        for statement in init_statements(&self.init_sql) {
            // The dialect is only known on connection: MySQL's also serves
            // MariaDB, which shares the syntax of settings.
            if let Some(refusal) = session_setting_refusal(&statement, Dialect::MySql) {
                return Err(ConnectionsError::Invalid(refused_setting(
                    statement, refusal,
                )));
            }
        }
        Ok(())
    }
}

/// On-connect queries, split as in the editor (`splitter` crate).
fn init_statements(init_sql: &str) -> Vec<String> {
    prisme_splitter::split(init_sql)
        .into_iter()
        .map(|statement| statement.sql)
        .collect()
}

/// Detail of an on-connect query refused, by the reason of its refusal.
fn refused_setting(statement: String, refusal: SettingRefusal) -> ErrorDetail {
    match refusal {
        SettingRefusal::NotUnderstood => ErrorDetail::InitSqlNotUnderstood { statement },
        SettingRefusal::Global => ErrorDetail::InitSqlGlobalSetting { statement },
        SettingRefusal::NotASetting => ErrorDetail::InitSqlNotSetting { statement },
    }
}

/// Checks that a field has between `min` and `max` characters (not bytes, like SQLite's `length()`).
fn check_length(field: Field, value: &str, min: usize, max: usize) -> Result<(), ConnectionsError> {
    let length = value.chars().count();
    if length < min || length > max {
        return Err(ConnectionsError::Invalid(ErrorDetail::FieldLength {
            field,
            min: count(min),
            max: count(max),
            length: count(length),
        }));
    }
    Ok(())
}

/// A number of characters, for an error detail (saturates beyond `u32::MAX`).
pub fn count(characters: usize) -> u32 {
    u32::try_from(characters).unwrap_or(u32::MAX)
}

/// Saves a connection for user `user_id` and returns its id.
pub fn insert(
    db: &Connection,
    key: &MasterKey,
    user_id: i64,
    new: &NewConnection,
) -> Result<i64, ConnectionsError> {
    new.validate()?;
    let password_secret = secrets::encrypt(key, &new.password)?;

    let result = db.execute(
        "INSERT INTO connections (user_id, name, engine, host, port, username, password_secret, database_name, init_sql, session_timeout_minutes, show_system_schemas)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            user_id,
            new.name,
            ENGINE_MYSQL,
            new.host,
            new.port,
            new.username,
            password_secret,
            new.database_name,
            new.init_sql,
            new.session_timeout_minutes,
            new.show_system_schemas,
        ],
    );
    result.map_err(|error| name_error(error, &new.name))?;
    Ok(db.last_insert_rowid())
}

/// Changes connection `id` of user `user_id`. An empty password keeps the
/// saved password: the interface does not know it, so it cannot send it back.
///
/// Returns `false` if the connection does not exist or belongs to another user.
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
             password_secret = COALESCE(?7, password_secret), database_name = ?8, init_sql = ?9,
             session_timeout_minutes = ?10, show_system_schemas = ?11
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
            changed.init_sql,
            changed.session_timeout_minutes,
            changed.show_system_schemas,
        ],
    );
    let updated = result.map_err(|error| name_error(error, &changed.name))?;
    Ok(updated == 1)
}

/// Translates a violation of the name's uniqueness into `DuplicateName`, lets the other errors through.
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

/// Lists the connections of user `user_id`, by name, without their password.
pub fn list(db: &Connection, user_id: i64) -> Result<Vec<ConnectionSummary>, ConnectionsError> {
    let mut statement = db.prepare(
        "SELECT id, name, engine, host, port, username, database_name, init_sql,
                session_timeout_minutes, show_system_schemas, server_flavor, server_version,
                created_at
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
            init_sql: row.get(7)?,
            session_timeout_minutes: row.get(8)?,
            show_system_schemas: row.get(9)?,
            server_flavor: flavor_of(&row.get::<_, String>(10)?),
            server_version: Some(row.get::<_, String>(11)?).filter(|version| !version.is_empty()),
            created_at: row.get(12)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Deletes connection `id` of user `user_id`.
///
/// Returns `false` if the connection does not exist or belongs to another user.
pub fn delete(db: &Connection, user_id: i64, id: i64) -> Result<bool, ConnectionsError> {
    let deleted = db.execute(
        "DELETE FROM connections WHERE id = ?1 AND user_id = ?2",
        params![id, user_id],
    )?;
    Ok(deleted == 1)
}

/// Statement that puts a connection in read-only mode: the database then
/// refuses the writes of its transactions (`docs/V1.md`, section 2.4).
///
/// The standard statement, known by every version of MySQL and MariaDB. The
/// variable it sets is named `transaction_read_only` in MySQL and from
/// MariaDB 11.1 only, `tx_read_only` before: `SET SESSION
/// transaction_read_only = ON` fails on MariaDB 10.x and 11.0 (error 1193).
const READ_ONLY_STATEMENT: &str = "SET SESSION TRANSACTION READ ONLY";

/// Adds the read-only mode to the initialisation of each connection of a query
/// session, when `read_only` is set. It comes last, after the statements of the
/// connection itself, so that they cannot switch it off.
pub fn with_read_only(mut target: MysqlTarget, read_only: bool) -> MysqlTarget {
    if read_only {
        target.init_statements.push(READ_ONLY_STATEMENT.to_string());
    }
    target
}

/// Loads the connection settings, password decrypted, of connection `id`.
///
/// Returns `None` if the connection does not exist or belongs to another user.
pub fn load_target(
    db: &Connection,
    key: &MasterKey,
    user_id: i64,
    id: i64,
) -> Result<Option<MysqlTarget>, ConnectionsError> {
    let row = db
        .query_row(
            "SELECT host, port, username, password_secret, database_name, init_sql
             FROM connections WHERE id = ?1 AND user_id = ?2",
            params![id, user_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u16>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()?;

    let Some((host, port, username, password_secret, database_name, init_sql)) = row else {
        return Ok(None);
    };
    Ok(Some(MysqlTarget {
        host,
        port,
        username,
        password: secrets::decrypt(key, &password_secret)?,
        database_name,
        init_statements: init_statements(&init_sql),
    }))
}

/// Name of connection `id` of user `user_id`, empty if it does not exist or
/// belongs to another user.
pub fn name_of(db: &Connection, user_id: i64, id: i64) -> Result<String, ConnectionsError> {
    let name = db
        .query_row(
            "SELECT name FROM connections WHERE id = ?1 AND user_id = ?2",
            params![id, user_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(name.unwrap_or_default())
}

/// Remembers the flavor and the version of the server of connection `id`,
/// seen on a successful connection.
pub fn remember_server(
    db: &Connection,
    user_id: i64,
    id: i64,
    server: &ServerInfo,
) -> Result<(), ConnectionsError> {
    let flavor = match server.flavor {
        Flavor::MySql => "mysql",
        Flavor::MariaDb => "mariadb",
    };
    db.execute(
        "UPDATE connections SET server_flavor = ?3, server_version = ?4
         WHERE id = ?1 AND user_id = ?2",
        params![id, user_id, flavor, server.version],
    )?;
    Ok(())
}

/// Saved flavor, `None` for the empty string (never connected).
fn flavor_of(name: &str) -> Option<Flavor> {
    match name {
        "mysql" => Some(Flavor::MySql),
        "mariadb" => Some(Flavor::MariaDb),
        _ => None,
    }
}

/// Keep-alive delay of the sessions of connection `id`, in minutes.
///
/// Returns `None` if the connection does not exist or belongs to another user.
pub fn session_timeout(
    db: &Connection,
    user_id: i64,
    id: i64,
) -> Result<Option<u32>, ConnectionsError> {
    let minutes = db
        .query_row(
            "SELECT session_timeout_minutes FROM connections WHERE id = ?1 AND user_id = ?2",
            params![id, user_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(minutes)
}

/// Connection settings of a test of typed settings. An empty password takes
/// that of the saved connection `saved_id`, if there is one.
///
/// Returns `None` if `saved_id` designates a missing connection or one
/// belonging to another user.
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
        init_statements: init_statements(&connection.init_sql),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store;
    use prisme_connectors::mysql::{connect, execute};
    use testcontainers_modules::mariadb::Mariadb;
    use testcontainers_modules::testcontainers::ImageExt;
    use testcontainers_modules::testcontainers::runners::AsyncRunner;

    fn new_connection(name: &str) -> NewConnection {
        NewConnection {
            name: name.to_string(),
            host: "localhost".to_string(),
            port: 3306,
            username: "root".to_string(),
            password: SecretString::from("secret"),
            database_name: String::new(),
            init_sql: String::new(),
            session_timeout_minutes: 5,
            show_system_schemas: true,
        }
    }

    #[test]
    fn keeps_session_settings_and_refuses_the_rest() {
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let user_id = store::ensure_user(&db, "test@example.com").unwrap();
        let mut connection = new_connection("with settings");
        connection.init_sql =
            "SET NAMES utf8mb4;\nSET SESSION sql_mode = 'ANSI_QUOTES';".to_string();
        let id = insert(&db, &key, user_id, &connection).unwrap();
        assert_eq!(list(&db, user_id).unwrap()[0].init_sql, connection.init_sql);
        let target = load_target(&db, &key, user_id, id).unwrap().unwrap();
        assert_eq!(
            target.init_statements,
            ["SET NAMES utf8mb4", "SET SESSION sql_mode = 'ANSI_QUOTES'"]
        );

        connection.init_sql = "SET NAMES utf8mb4; SET GLOBAL max_connections = 1".to_string();
        assert!(matches!(
            update(&db, &key, user_id, id, &connection),
            Err(ConnectionsError::Invalid(_))
        ));
        connection.init_sql = "DELETE FROM t".to_string();
        assert!(matches!(
            insert(&db, &key, user_id, &connection),
            Err(ConnectionsError::Invalid(_))
        ));
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
        // The same name stays free for another user.
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

        let mut changed = new_connection("staging");
        changed.password = SecretString::from("");
        assert!(update(&db, &key, alice, id, &changed).unwrap());
        assert_eq!(list(&db, alice).unwrap()[0].name, "staging");
        let target = load_target(&db, &key, alice, id).unwrap().unwrap();
        assert_eq!(target.password.expose_secret(), "secret");

        changed.password = SecretString::from("new");
        assert!(update(&db, &key, alice, id, &changed).unwrap());
        let target = load_target(&db, &key, alice, id).unwrap().unwrap();
        assert_eq!(target.password.expose_secret(), "new");

        assert!(!update(&db, &key, bob, id, &changed).unwrap());
    }

    #[test]
    fn update_rejects_a_name_already_taken() {
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();
        insert(&db, &key, user_id, &new_connection("prod")).unwrap();
        let id = insert(&db, &key, user_id, &new_connection("staging")).unwrap();

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
        let target = probe_target(&db, &key, alice, request("other", Some(id)));
        assert_eq!(target.unwrap().unwrap().password.expose_secret(), "other");
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

    #[test]
    fn remembers_the_server() {
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();
        let id = insert(&db, &key, user_id, &new_connection("prod")).unwrap();
        let summary = &list(&db, user_id).unwrap()[0];
        assert_eq!(
            (summary.server_flavor, &summary.server_version),
            (None, &None)
        );
        let server = ServerInfo {
            flavor: Flavor::MariaDb,
            version: "10.11.11-MariaDB".to_string(),
            ansi_quotes: false,
        };
        remember_server(&db, user_id, id, &server).unwrap();
        let summary = &list(&db, user_id).unwrap()[0];
        assert_eq!(summary.server_flavor, Some(Flavor::MariaDb));
        assert_eq!(summary.server_version.as_deref(), Some("10.11.11-MariaDB"));
    }

    #[test]
    fn keeps_the_session_timeout_within_a_day() {
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();
        let mut connection = new_connection("prod");
        connection.session_timeout_minutes = 1441;
        assert!(connection.validate().is_err());

        connection.session_timeout_minutes = 30;
        let id = insert(&db, &key, user_id, &connection).unwrap();
        assert_eq!(list(&db, user_id).unwrap()[0].session_timeout_minutes, 30);
        assert!(list(&db, user_id).unwrap()[0].show_system_schemas);
        assert_eq!(session_timeout(&db, user_id, id).unwrap(), Some(30));
        assert_eq!(session_timeout(&db, user_id + 1, id).unwrap(), None);
    }

    #[test]
    fn read_only_comes_last_and_only_when_asked() {
        let target = MysqlTarget {
            host: "127.0.0.1".to_string(),
            port: 3306,
            username: "root".to_string(),
            password: SecretString::from("secret"),
            database_name: String::new(),
            init_statements: vec!["SET NAMES utf8mb4".to_string()],
        };
        let read_only = with_read_only(target.clone(), true);
        assert_eq!(
            read_only.init_statements,
            ["SET NAMES utf8mb4", READ_ONLY_STATEMENT]
        );
        assert_eq!(
            with_read_only(target, false).init_statements,
            ["SET NAMES utf8mb4"]
        );
    }

    #[tokio::test]
    #[ignore = "requires Docker Desktop running"]
    async fn read_only_session_refuses_writes_on_mariadb_before_11_1() {
        // MariaDB 10.6 knows `tx_read_only`, not `transaction_read_only`.
        let server = Mariadb::default()
            .with_tag("10.6.28")
            .start()
            .await
            .unwrap();
        let target = MysqlTarget {
            host: server.get_host().await.unwrap().to_string(),
            port: server.get_host_port_ipv4(3306).await.unwrap(),
            username: "root".to_string(),
            password: SecretString::from(""),
            database_name: "test".to_string(),
            init_statements: Vec::new(),
        };
        let mut writer = connect(&target).await.unwrap();
        execute(&mut writer, "CREATE TABLE t (a INT)")
            .await
            .unwrap()
            .discard()
            .await;

        let mut reader = connect(&with_read_only(target, true)).await.unwrap();
        execute(&mut reader, "SELECT a FROM t")
            .await
            .unwrap()
            .discard()
            .await;
        let Err(error) = execute(&mut reader, "INSERT INTO t VALUES (1)").await else {
            panic!("the write went through a read-only session");
        };
        assert!(error.to_string().contains("READ ONLY"), "{error}");
    }
}
