//! Saved connections to the sources.
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
//! page is closed (see `sessions.rs`), and remembers the flavor and the
//! version of the source seen on the last successful connection, with its
//! date.
//!
//! The settings every engine shares are in `connections`, those of each
//! family of settings in its own table (`connection_servers` for the
//! servers), the last successful connection in `connection_probes`
//! (`migrations/0037_connections_by_family.sql`). The API carries the
//! settings tagged by their family (`"family": "server"`); the engine of a
//! saved connection does not change.

use prisme_connectors::api::{Engine, Family, ServerSettings, Settings, SourceInfo, Target};
use prisme_connectors::flavor::Flavor;
use prisme_dialects::Dialect;
use prisme_dialects::classify::{SettingRefusal, session_setting_refusal};
use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row, ffi, params};
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

/// Maximum length of the version of a source kept, in characters.
const VERSION_MAX_LENGTH: usize = 100;

/// Maximum keep-alive delay of a detached session, in minutes (24 hours).
const SESSION_TIMEOUT_MAX_MINUTES: u32 = 1440;

/// Keep-alive delay of a detached session, in minutes, when the input gives none.
const SESSION_TIMEOUT_DEFAULT_MINUTES: u32 = 5;

/// Connection to save, as typed by the user.
#[derive(Deserialize, TS)]
#[ts(export)]
pub struct NewConnection {
    /// Name chosen by the user, unique among their connections.
    pub name: String,
    /// Engine of the source: its family is that of the settings.
    pub engine: Engine,
    /// Settings of the source, by family.
    pub settings: NewSettings,
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
    /// Does the navigator show the system namespaces (`information_schema`,
    /// `mysql`...)? Absent: yes.
    #[serde(default = "default_show_system_schemas")]
    #[ts(optional, as = "Option<bool>")]
    pub show_system_schemas: bool,
}

/// Settings of a source to save, by family of settings.
#[derive(Deserialize, TS)]
#[ts(export)]
#[serde(tag = "family", rename_all = "snake_case")]
pub enum NewSettings {
    /// A server reached over the network.
    Server(NewServerSettings),
}

impl NewSettings {
    /// Family of the settings.
    fn family(&self) -> Family {
        match self {
            Self::Server(_) => Family::Server,
        }
    }
}

/// Settings of a server to save, as typed by the user.
#[derive(Deserialize, TS)]
#[ts(export)]
pub struct NewServerSettings {
    /// Host name or IP address of the server.
    pub host: String,
    /// TCP port of the server.
    pub port: u16,
    /// Connection account.
    pub username: String,
    /// Password of the account, encrypted before storage. Empty for a saved
    /// connection: unchanged, or that of the copied connection.
    #[ts(type = "string")]
    pub password: SecretString,
    /// Database selected on connection. Absent or empty: none.
    #[serde(default)]
    #[ts(optional, as = "Option<String>")]
    pub database_name: String,
}

/// Default display of the system namespaces, for deserialising `NewConnection`.
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
    /// Engine of the source.
    pub engine: Engine,
    /// Settings of the source, by family, without the password.
    pub settings: SavedSettings,
    /// Queries run each time a connection opens, empty string if none.
    pub init_sql: String,
    /// Keep-alive delay of a session after its page is closed, in minutes.
    pub session_timeout_minutes: u32,
    /// Does the navigator show the system namespaces?
    pub show_system_schemas: bool,
    /// Last successful connection to the source, `null` if the connection
    /// never succeeded.
    pub probe: Option<ConnectionProbe>,
    /// Date it was saved, in UTC, in ISO 8601 format.
    pub created_at: String,
}

/// Saved settings of a source, by family of settings, without secrets.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(tag = "family", rename_all = "snake_case")]
pub enum SavedSettings {
    /// A server reached over the network.
    Server(SavedServerSettings),
}

/// Saved settings of a server, without its password.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct SavedServerSettings {
    /// Host name or IP address of the server.
    pub host: String,
    /// TCP port of the server.
    pub port: u16,
    /// Connection account.
    pub username: String,
    /// Database selected on connection, empty string if none.
    pub database_name: String,
}

/// What the last successful connection to a source detected.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ConnectionProbe {
    /// Flavor of the source.
    pub flavor: Flavor,
    /// Version of the source, as it gives it (`SELECT VERSION()` for MySQL).
    pub version: String,
    /// Date of the connection, in UTC, in ISO 8601 format.
    pub connected_at: String,
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
    /// Checks the input, with the same limits as the tables of the
    /// connections.
    pub fn validate(&self) -> Result<(), ConnectionsError> {
        check_length(Field::Name, &self.name, 1, NAME_MAX_LENGTH)?;
        if self.engine.family() != self.settings.family() {
            return Err(ConnectionsError::Invalid(ErrorDetail::SettingsFamily {
                engine: self.engine.key().to_string(),
            }));
        }
        match &self.settings {
            NewSettings::Server(server) => server.validate()?,
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

impl NewServerSettings {
    /// Checks the settings, with the same limits as `connection_servers`.
    fn validate(&self) -> Result<(), ConnectionsError> {
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
        Ok(())
    }
}

/// On-connect queries, split as in the editor (`splitter` crate).
fn init_statements(init_sql: &str) -> Vec<String> {
    prisme_splitter::split(init_sql, Dialect::MySql.lexical_rules())
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
    let password_secret = match &new.settings {
        NewSettings::Server(server) => secrets::encrypt(key, &server.password)?,
    };
    insert_rows(db, user_id, new, &password_secret)
}

/// Saves a copy of connection `source_id` of user `user_id`, with the
/// settings of `new`, and returns its id. An empty password takes that of the
/// source: the interface does not know it, so it cannot send it. The secret
/// is copied as stored, without being decrypted.
///
/// Returns `None` if the source does not exist, belongs to another user, or
/// is of another engine.
pub fn insert_copy(
    db: &Connection,
    key: &MasterKey,
    user_id: i64,
    source_id: i64,
    new: &NewConnection,
) -> Result<Option<i64>, ConnectionsError> {
    new.validate()?;
    let NewSettings::Server(server) = &new.settings;
    let saved_secret: Option<Vec<u8>> = db
        .query_row(
            "SELECT s.password_secret
             FROM connections c JOIN connection_servers s ON s.connection_id = c.id
             WHERE c.id = ?1 AND c.user_id = ?2 AND c.engine = ?3",
            params![source_id, user_id, new.engine.key()],
            |row| row.get(0),
        )
        .optional()?;
    let Some(saved_secret) = saved_secret else {
        return Ok(None);
    };
    let password_secret = if server.password.expose_secret().is_empty() {
        saved_secret
    } else {
        secrets::encrypt(key, &server.password)?
    };
    insert_rows(db, user_id, new, &password_secret).map(Some)
}

/// Inserts the rows of a validated connection, its password already
/// encrypted, and returns its id.
fn insert_rows(
    db: &Connection,
    user_id: i64,
    new: &NewConnection,
    password_secret: &[u8],
) -> Result<i64, ConnectionsError> {
    let transaction = db.unchecked_transaction()?;
    let result = transaction.execute(
        "INSERT INTO connections (user_id, name, engine, init_sql, session_timeout_minutes, show_system_schemas)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            user_id,
            new.name,
            new.engine.key(),
            new.init_sql,
            new.session_timeout_minutes,
            new.show_system_schemas,
        ],
    );
    result.map_err(|error| name_error(error, &new.name))?;
    let id = transaction.last_insert_rowid();
    match &new.settings {
        NewSettings::Server(server) => {
            transaction.execute(
                "INSERT INTO connection_servers (connection_id, host, port, username, password_secret, database_name)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    id,
                    server.host,
                    server.port,
                    server.username,
                    password_secret,
                    server.database_name,
                ],
            )?;
        }
    }
    transaction.commit()?;
    Ok(id)
}

/// Changes connection `id` of user `user_id`. An empty password keeps the
/// saved password: the interface does not know it, so it cannot send it back.
///
/// Returns `false` if the connection does not exist, belongs to another user,
/// or is of another engine.
pub fn update(
    db: &Connection,
    key: &MasterKey,
    user_id: i64,
    id: i64,
    changed: &NewConnection,
) -> Result<bool, ConnectionsError> {
    changed.validate()?;
    let transaction = db.unchecked_transaction()?;
    let result = transaction.execute(
        "UPDATE connections
         SET name = ?4, init_sql = ?5, session_timeout_minutes = ?6, show_system_schemas = ?7
         WHERE id = ?1 AND user_id = ?2 AND engine = ?3",
        params![
            id,
            user_id,
            changed.engine.key(),
            changed.name,
            changed.init_sql,
            changed.session_timeout_minutes,
            changed.show_system_schemas,
        ],
    );
    let updated = result.map_err(|error| name_error(error, &changed.name))?;
    if updated != 1 {
        return Ok(false);
    }
    match &changed.settings {
        NewSettings::Server(server) => {
            let password_secret = if server.password.expose_secret().is_empty() {
                None
            } else {
                Some(secrets::encrypt(key, &server.password)?)
            };
            transaction.execute(
                "UPDATE connection_servers
                 SET host = ?2, port = ?3, username = ?4,
                     password_secret = COALESCE(?5, password_secret), database_name = ?6
                 WHERE connection_id = ?1",
                params![
                    id,
                    server.host,
                    server.port,
                    server.username,
                    password_secret,
                    server.database_name,
                ],
            )?;
        }
    }
    transaction.commit()?;
    Ok(true)
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

/// Engine of the key read in column `index`.
fn engine_at(row: &Row<'_>, index: usize) -> rusqlite::Result<Engine> {
    let key: String = row.get(index)?;
    // The foreign key of the column keeps the engines of the reference
    // table; one this version of Prisme does not know is an error.
    Engine::from_key(&key).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            Type::Text,
            format!("unknown engine {key}").into(),
        )
    })
}

/// Lists the connections of user `user_id`, by name, without their password.
pub fn list(db: &Connection, user_id: i64) -> Result<Vec<ConnectionSummary>, ConnectionsError> {
    let mut statement = db.prepare(
        "SELECT c.id, c.name, c.engine, s.host, s.port, s.username, s.database_name, c.init_sql,
                c.session_timeout_minutes, c.show_system_schemas, p.flavor, p.version,
                p.connected_at, c.created_at
         FROM connections c
         JOIN connection_servers s ON s.connection_id = c.id
         LEFT JOIN connection_probes p ON p.connection_id = c.id
         WHERE c.user_id = ?1 ORDER BY c.name",
    )?;
    let rows = statement.query_map(params![user_id], |row| {
        let probe = match row.get::<_, Option<String>>(10)? {
            Some(flavor) => Flavor::from_key(&flavor)
                .map(|flavor| -> rusqlite::Result<ConnectionProbe> {
                    Ok(ConnectionProbe {
                        flavor,
                        version: row.get(11)?,
                        connected_at: row.get(12)?,
                    })
                })
                .transpose()?,
            None => None,
        };
        Ok(ConnectionSummary {
            id: row.get(0)?,
            name: row.get(1)?,
            engine: engine_at(row, 2)?,
            settings: SavedSettings::Server(SavedServerSettings {
                host: row.get(3)?,
                port: row.get(4)?,
                username: row.get(5)?,
                database_name: row.get(6)?,
            }),
            init_sql: row.get(7)?,
            session_timeout_minutes: row.get(8)?,
            show_system_schemas: row.get(9)?,
            probe,
            created_at: row.get(13)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Deletes connection `id` of user `user_id`, with its settings and its probe.
///
/// Returns `false` if the connection does not exist or belongs to another user.
pub fn delete(db: &Connection, user_id: i64, id: i64) -> Result<bool, ConnectionsError> {
    let deleted = db.execute(
        "DELETE FROM connections WHERE id = ?1 AND user_id = ?2",
        params![id, user_id],
    )?;
    Ok(deleted == 1)
}

/// Loads the settings of the source of connection `id`, password decrypted.
///
/// Returns `None` if the connection does not exist or belongs to another user.
pub fn load_target(
    db: &Connection,
    key: &MasterKey,
    user_id: i64,
    id: i64,
) -> Result<Option<Target>, ConnectionsError> {
    let row = db
        .query_row(
            "SELECT c.engine, s.host, s.port, s.username, s.password_secret, s.database_name,
                    c.init_sql
             FROM connections c JOIN connection_servers s ON s.connection_id = c.id
             WHERE c.id = ?1 AND c.user_id = ?2",
            params![id, user_id],
            |row| {
                Ok((
                    engine_at(row, 0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u16>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .optional()?;

    let Some((engine, host, port, username, password_secret, database_name, init_sql)) = row else {
        return Ok(None);
    };
    Ok(Some(Target {
        engine,
        settings: Settings::Server(ServerSettings {
            host,
            port,
            username,
            password: secrets::decrypt(key, &password_secret)?,
            database_name,
        }),
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

/// Remembers the flavor and the version of the source of connection `id`,
/// seen on a successful connection, and the date of that connection.
pub fn remember_server(
    db: &Connection,
    user_id: i64,
    id: i64,
    server: &SourceInfo,
) -> Result<(), ConnectionsError> {
    let version: String = server.version.chars().take(VERSION_MAX_LENGTH).collect();
    db.execute(
        "INSERT INTO connection_probes (connection_id, flavor, version)
         SELECT id, ?3, ?4 FROM connections WHERE id = ?1 AND user_id = ?2
         ON CONFLICT (connection_id) DO UPDATE
         SET flavor = excluded.flavor, version = excluded.version,
             connected_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        params![id, user_id, server.flavor.key(), version],
    )?;
    Ok(())
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
) -> Result<Option<Target>, ConnectionsError> {
    let ProbeRequest {
        connection,
        saved_id,
    } = request;
    let NewSettings::Server(server) = connection.settings;
    let password = match saved_id {
        Some(id) if server.password.expose_secret().is_empty() => {
            match load_target(db, key, user_id, id)? {
                Some(Target {
                    settings: Settings::Server(saved),
                    ..
                }) => saved.password,
                // A saved connection of another family has no password.
                Some(_) | None => return Ok(None),
            }
        }
        _ => server.password,
    };
    Ok(Some(Target {
        engine: connection.engine,
        settings: Settings::Server(ServerSettings {
            host: server.host,
            port: server.port,
            username: server.username,
            password,
            database_name: server.database_name,
        }),
        init_statements: init_statements(&connection.init_sql),
    }))
}

#[cfg(test)]
mod tests {
    use prisme_connectors::api::DialectOptions;
    use prisme_connectors::mysql::capabilities;

    use super::*;
    use crate::store;

    /// Password of the settings of a server.
    fn password(target: &Target) -> &str {
        let Settings::Server(server) = &target.settings else {
            panic!("settings of a server expected");
        };
        server.password.expose_secret()
    }

    fn new_connection(name: &str) -> NewConnection {
        NewConnection {
            name: name.to_string(),
            engine: Engine::Mysql,
            settings: NewSettings::Server(NewServerSettings {
                host: "localhost".to_string(),
                port: 3306,
                username: "root".to_string(),
                password: SecretString::from("secret"),
                database_name: String::new(),
            }),
            init_sql: String::new(),
            session_timeout_minutes: 5,
            show_system_schemas: true,
        }
    }

    /// Settings of the server of a connection to save.
    fn server(connection: &mut NewConnection) -> &mut NewServerSettings {
        let NewSettings::Server(server) = &mut connection.settings;
        server
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
        assert_eq!(summaries[0].engine, Engine::Mysql);
        let SavedSettings::Server(saved) = &summaries[0].settings;
        assert_eq!((saved.host.as_str(), saved.port), ("localhost", 3306));

        let target = load_target(&db, &key, user_id, id).unwrap().unwrap();
        assert_eq!(password(&target), "secret");
    }

    #[test]
    fn password_is_stored_encrypted() {
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();
        insert(&db, &key, user_id, &new_connection("prod")).unwrap();

        let stored: Vec<u8> = db
            .query_row(
                "SELECT password_secret FROM connection_servers",
                [],
                |row| row.get(0),
            )
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
        // The settings of the server go with the connection.
        let servers: i64 = db
            .query_row("SELECT COUNT(*) FROM connection_servers", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(servers, 0);
    }

    #[test]
    fn update_keeps_the_password_when_left_empty() {
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let alice = store::ensure_user(&db, "alice@example.com").unwrap();
        let bob = store::ensure_user(&db, "bob@example.com").unwrap();
        let id = insert(&db, &key, alice, &new_connection("prod")).unwrap();

        let mut changed = new_connection("staging");
        server(&mut changed).password = SecretString::from("");
        assert!(update(&db, &key, alice, id, &changed).unwrap());
        assert_eq!(list(&db, alice).unwrap()[0].name, "staging");
        let target = load_target(&db, &key, alice, id).unwrap().unwrap();
        assert_eq!(password(&target), "secret");

        server(&mut changed).password = SecretString::from("new");
        assert!(update(&db, &key, alice, id, &changed).unwrap());
        let target = load_target(&db, &key, alice, id).unwrap().unwrap();
        assert_eq!(password(&target), "new");

        assert!(!update(&db, &key, bob, id, &changed).unwrap());
    }

    #[test]
    fn copy_takes_the_source_password_when_left_empty() {
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let alice = store::ensure_user(&db, "alice@example.com").unwrap();
        let bob = store::ensure_user(&db, "bob@example.com").unwrap();
        let source = insert(&db, &key, alice, &new_connection("prod")).unwrap();

        let mut copy = new_connection("prod (copie)");
        server(&mut copy).password = SecretString::from("");
        let id = insert_copy(&db, &key, alice, source, &copy)
            .unwrap()
            .unwrap();
        let target = load_target(&db, &key, alice, id).unwrap().unwrap();
        assert_eq!(password(&target), "secret");

        copy.name = "prod (autre)".to_string();
        server(&mut copy).password = SecretString::from("other");
        let id = insert_copy(&db, &key, alice, source, &copy)
            .unwrap()
            .unwrap();
        let target = load_target(&db, &key, alice, id).unwrap().unwrap();
        assert_eq!(password(&target), "other");

        // The name stays unique, and another user's connection cannot be copied.
        assert!(matches!(
            insert_copy(&db, &key, alice, source, &copy),
            Err(ConnectionsError::DuplicateName(_))
        ));
        copy.name = "bob".to_string();
        assert!(
            insert_copy(&db, &key, bob, source, &copy)
                .unwrap()
                .is_none()
        );
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
            server(&mut connection).password = SecretString::from(password);
            ProbeRequest {
                connection,
                saved_id,
            }
        };

        let target = probe_target(&db, &key, alice, request("", Some(id)));
        assert_eq!(password(&target.unwrap().unwrap()), "secret");
        let target = probe_target(&db, &key, alice, request("other", Some(id)));
        assert_eq!(password(&target.unwrap().unwrap()), "other");
        let target = probe_target(&db, &key, alice, request("", None));
        assert_eq!(password(&target.unwrap().unwrap()), "");
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
        server(&mut connection).port = 0;
        assert!(connection.validate().is_err());
    }

    #[test]
    fn remembers_the_server() {
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();
        let id = insert(&db, &key, user_id, &new_connection("prod")).unwrap();
        assert!(list(&db, user_id).unwrap()[0].probe.is_none());
        let server = SourceInfo {
            flavor: Flavor::MariaDb,
            version: "10.11.11-MariaDB".to_string(),
            dialect_options: DialectOptions::default(),
            oldest_supported: None,
            capabilities: capabilities(),
            databases: Vec::new(),
        };
        remember_server(&db, user_id, id, &server).unwrap();
        let summary = &list(&db, user_id).unwrap()[0];
        let probe = summary.probe.as_ref().unwrap();
        assert_eq!(probe.flavor, Flavor::MariaDb);
        assert_eq!(probe.version, "10.11.11-MariaDB");
        assert!(probe.connected_at.ends_with('Z'), "{}", probe.connected_at);

        // A later connection replaces the probe; another user's changes nothing.
        let server = SourceInfo {
            flavor: Flavor::MySql,
            version: "8".repeat(VERSION_MAX_LENGTH + 10),
            ..server
        };
        remember_server(&db, user_id + 1, id, &server).unwrap();
        let probe = |db: &Connection| list(db, user_id).unwrap().remove(0).probe.unwrap();
        assert_eq!(probe(&db).flavor, Flavor::MariaDb);
        remember_server(&db, user_id, id, &server).unwrap();
        assert_eq!(probe(&db).flavor, Flavor::MySql);
        assert_eq!(probe(&db).version, "8".repeat(VERSION_MAX_LENGTH));
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
}
