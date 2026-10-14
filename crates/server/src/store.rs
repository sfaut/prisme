//! Base interne de Prisme : un fichier SQLite en mode WAL dans le dossier de données.
//!
//! Elle conserve les utilisateurs et les connexions, puis les notebooks. Le
//! schéma évolue par migrations SQL numérotées (dossier `migrations/`),
//! appliquées à l'ouverture. `rusqlite` est synchrone : hors du démarrage, les
//! accès passeront par des tâches bloquantes dédiées (`spawn_blocking`).

use std::path::Path;
use std::sync::LazyLock;

use rusqlite::{Connection, params};
use rusqlite_migration::{M, Migrations};

/// Nom du fichier de la base interne dans le dossier de données.
pub const FILE_NAME: &str = "prisme.db";

/// Migrations du schéma, dans l'ordre. Une migration publiée ne se modifie plus :
/// tout changement passe par une nouvelle migration ajoutée en fin de liste.
static MIGRATIONS: LazyLock<Migrations<'static>> = LazyLock::new(|| {
    Migrations::new(vec![
        M::up(include_str!("../migrations/0001_users.sql")),
        M::up(include_str!("../migrations/0002_connections.sql")),
        M::up(include_str!("../migrations/0003_users_row_limit.sql")),
        M::up(include_str!("../migrations/0004_connections_init_sql.sql")),
        M::up(include_str!("../migrations/0005_editors.sql")),
    ])
});

/// Erreurs d'accès à la base interne.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// Erreur SQLite.
    #[error("base interne : {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Échec d'une migration du schéma.
    #[error("migration de la base interne : {0}")]
    Migration(#[from] rusqlite_migration::Error),
}

/// Ouvre (ou crée) la base interne au chemin `path` et met son schéma à jour.
pub fn open(path: &Path) -> Result<Connection, StoreError> {
    let mut connection = Connection::open(path)?;
    // WAL : les lectures ne bloquent pas l'écriture, et inversement.
    connection.pragma_update(None, "journal_mode", "WAL")?;
    prepare(&mut connection)?;
    Ok(connection)
}

/// Active les clefs étrangères et applique les migrations en attente.
fn prepare(connection: &mut Connection) -> Result<(), StoreError> {
    // SQLite ignore les clefs étrangères tant que ce réglage n'est pas activé, connexion par connexion.
    connection.pragma_update(None, "foreign_keys", "ON")?;
    MIGRATIONS.to_latest(connection)?;
    Ok(())
}

/// Crée l'utilisateur `email` s'il n'existe pas encore, et renvoie son identifiant.
pub fn ensure_user(connection: &Connection, email: &str) -> Result<i64, StoreError> {
    connection.execute(
        "INSERT INTO users (email) VALUES (?1) ON CONFLICT (email) DO NOTHING",
        params![email],
    )?;
    let id = connection.query_row(
        "SELECT id FROM users WHERE email = ?1",
        params![email],
        |row| row.get(0),
    )?;
    Ok(id)
}

/// Ouvre une base interne en mémoire, schéma à jour, pour les tests.
#[cfg(test)]
pub fn open_in_memory() -> Connection {
    let mut connection = Connection::open_in_memory().unwrap();
    prepare(&mut connection).unwrap();
    connection
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_valid() {
        MIGRATIONS.validate().unwrap();
    }

    #[test]
    fn ensure_user_is_idempotent() {
        let connection = open_in_memory();
        let first = ensure_user(&connection, "sebastien.faut@gmail.com").unwrap();
        let second = ensure_user(&connection, "sebastien.faut@gmail.com").unwrap();

        assert_eq!(first, second);
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn rejects_too_long_email() {
        let connection = open_in_memory();
        let email = format!("{}@example.com", "a".repeat(250));
        assert!(ensure_user(&connection, &email).is_err());
    }

    #[test]
    fn file_store_uses_wal() {
        let dir = tempfile::tempdir().unwrap();
        let connection = open(&dir.path().join(FILE_NAME)).unwrap();
        let mode: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
    }
}
