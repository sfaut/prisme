//! Catalogue d'un serveur MySQL ou MariaDB : bases, tables et colonnes.
//!
//! Sert à l'explorateur d'objets. Tout est lu dans `information_schema`, avec
//! des requêtes préparées : les noms de base et de table viennent de
//! l'interface et ne sont jamais concaténés au texte SQL.

use mysql_async::prelude::Queryable;
use serde::Serialize;
use ts_rs::TS;

use super::{Connection, MysqlError};

/// Table ou vue d'une base.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TableInfo {
    /// Nom de la table.
    pub name: String,
    /// Nature de l'objet.
    pub kind: TableKind,
    /// Nombre de lignes estimé par le serveur (exact pour MyISAM, approché pour
    /// InnoDB), `None` pour une vue.
    pub estimated_rows: Option<u64>,
}

/// Nature d'un objet de type table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum TableKind {
    /// Table de données.
    Table,
    /// Vue.
    View,
    /// Table ou vue fournie par le serveur lui-même (`information_schema`...).
    System,
}

/// Colonne d'une table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ColumnInfo {
    /// Nom de la colonne.
    pub name: String,
    /// Type complet, tel que déclaré (`varchar(50)`, `int unsigned`...).
    pub column_type: String,
    /// La colonne accepte-t-elle NULL ?
    pub nullable: bool,
    /// Rôle dans les index : `PRI`, `UNI`, `MUL`, ou chaîne vide.
    pub key: String,
}

/// Liste les bases du serveur, par nom.
pub async fn schemas(connection: &mut Connection) -> Result<Vec<String>, MysqlError> {
    Ok(connection
        .query("SELECT SCHEMA_NAME FROM information_schema.SCHEMATA ORDER BY SCHEMA_NAME")
        .await?)
}

/// Liste les tables et vues de la base `schema`, par nom.
pub async fn tables(
    connection: &mut Connection,
    schema: &str,
) -> Result<Vec<TableInfo>, MysqlError> {
    let rows: Vec<(String, String, Option<u64>)> = connection
        .exec(
            "SELECT TABLE_NAME, TABLE_TYPE, TABLE_ROWS FROM information_schema.TABLES
             WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME",
            (schema,),
        )
        .await?;
    Ok(rows
        .into_iter()
        .map(|(name, table_type, estimated_rows)| TableInfo {
            name,
            kind: table_kind(&table_type),
            estimated_rows,
        })
        .collect())
}

/// Liste les colonnes de la table `schema.table`, dans l'ordre de déclaration.
pub async fn columns(
    connection: &mut Connection,
    schema: &str,
    table: &str,
) -> Result<Vec<ColumnInfo>, MysqlError> {
    let rows: Vec<(String, String, String, String)> = connection
        .exec(
            "SELECT COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_KEY FROM information_schema.COLUMNS
             WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION",
            (schema, table),
        )
        .await?;
    Ok(rows
        .into_iter()
        .map(|(name, column_type, is_nullable, key)| ColumnInfo {
            name,
            column_type,
            nullable: is_nullable == "YES",
            key,
        })
        .collect())
}

/// Traduit la colonne `TABLE_TYPE` d'`information_schema.TABLES`.
fn table_kind(table_type: &str) -> TableKind {
    match table_type {
        "BASE TABLE" => TableKind::Table,
        "VIEW" => TableKind::View,
        // SYSTEM VIEW (MySQL, MariaDB), SYSTEM VERSIONED (MariaDB, table versionnée)...
        "SYSTEM VERSIONED" => TableKind::Table,
        _ => TableKind::System,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_table_types() {
        assert_eq!(table_kind("BASE TABLE"), TableKind::Table);
        assert_eq!(table_kind("SYSTEM VERSIONED"), TableKind::Table);
        assert_eq!(table_kind("VIEW"), TableKind::View);
        assert_eq!(table_kind("SYSTEM VIEW"), TableKind::System);
    }
}
