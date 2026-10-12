//! Catalogue d'un serveur MySQL ou MariaDB : bases, tables et colonnes.
//!
//! Sert à l'explorateur d'objets, qui affiche les éléments dès qu'ils
//! arrivent : d'abord les noms des bases, puis les tables base par base, puis
//! les colonnes base par base. Chaque lecture filtre sur une base, ce qui
//! évite au serveur de parcourir toutes les tables à la fois.
//!
//! Les requêtes ne demandent que des informations du dictionnaire (noms,
//! types) : demander aussi le nombre de lignes (`TABLE_ROWS`) obligerait le
//! serveur à ouvrir chaque table, ce qui est très lent sur un gros serveur.
//! Les noms de base sont passés en paramètres de requêtes préparées, jamais
//! concaténés au texte SQL.

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

/// Colonnes d'une table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TableColumns {
    /// Nom de la table.
    pub table: String,
    /// Colonnes, dans l'ordre de déclaration.
    pub columns: Vec<ColumnInfo>,
}

/// Ligne d'`information_schema.COLUMNS` : table, colonne, type, nullabilité, index.
type ColumnRow = (String, String, String, String, String);

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
    let rows: Vec<(String, String)> = connection
        .exec(
            "SELECT TABLE_NAME, TABLE_TYPE FROM information_schema.TABLES
             WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME",
            (schema,),
        )
        .await?;
    Ok(rows
        .into_iter()
        .map(|(name, table_type)| TableInfo {
            name,
            kind: table_kind(&table_type),
        })
        .collect())
}

/// Liste les colonnes de toutes les tables de la base `schema`, table par table.
pub async fn columns(
    connection: &mut Connection,
    schema: &str,
) -> Result<Vec<TableColumns>, MysqlError> {
    let rows: Vec<ColumnRow> = connection
        .exec(
            "SELECT TABLE_NAME, COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_KEY
             FROM information_schema.COLUMNS
             WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME, ORDINAL_POSITION",
            (schema,),
        )
        .await?;
    Ok(group_by_table(rows))
}

/// Regroupe les colonnes par table. Les lignes arrivent triées par table, puis
/// dans l'ordre de déclaration des colonnes.
fn group_by_table(rows: Vec<ColumnRow>) -> Vec<TableColumns> {
    let mut tables: Vec<TableColumns> = Vec::new();
    for (table, name, column_type, is_nullable, key) in rows {
        let column = ColumnInfo {
            name,
            column_type,
            nullable: is_nullable == "YES",
            key,
        };
        match tables.last_mut() {
            Some(last) if last.table == table => last.columns.push(column),
            _ => tables.push(TableColumns {
                table,
                columns: vec![column],
            }),
        }
    }
    tables
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
    fn groups_columns_by_table() {
        let row = |table: &str, name: &str| {
            (
                table.to_string(),
                name.to_string(),
                "int".to_string(),
                "NO".to_string(),
                String::new(),
            )
        };
        let tables = group_by_table(vec![row("t", "id"), row("t", "label"), row("u", "id")]);

        let summary: Vec<_> = tables
            .iter()
            .map(|table| (table.table.as_str(), table.columns.len()))
            .collect();
        assert_eq!(summary, [("t", 2), ("u", 1)]);
        assert_eq!(tables[0].columns[1].name, "label");
    }

    #[test]
    fn translates_table_types() {
        assert_eq!(table_kind("BASE TABLE"), TableKind::Table);
        assert_eq!(table_kind("SYSTEM VERSIONED"), TableKind::Table);
        assert_eq!(table_kind("VIEW"), TableKind::View);
        assert_eq!(table_kind("SYSTEM VIEW"), TableKind::System);
    }
}
