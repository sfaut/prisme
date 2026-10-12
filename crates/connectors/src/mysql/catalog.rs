//! Catalogue d'un serveur MySQL ou MariaDB : bases, tables et colonnes.
//!
//! Sert à l'explorateur d'objets, qui l'affiche en entier dès qu'une
//! connexion est choisie. Tout le catalogue se lit en trois requêtes sur
//! `information_schema` (bases, tables, colonnes), sur une seule connexion :
//! c'est bien plus rapide qu'une connexion par nœud ouvert dans l'arbre.

use std::collections::BTreeMap;

use mysql_async::prelude::Queryable;
use serde::Serialize;
use ts_rs::TS;

use super::{Connection, MysqlError};

/// Base du serveur, avec ses tables.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct SchemaInfo {
    /// Nom de la base.
    pub name: String,
    /// Tables et vues de la base, par nom.
    pub tables: Vec<TableInfo>,
}

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
    /// Colonnes, dans l'ordre de déclaration.
    pub columns: Vec<ColumnInfo>,
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

/// Ligne d'`information_schema.TABLES` : base, table, type, lignes estimées.
type TableRow = (String, String, String, Option<u64>);

/// Ligne d'`information_schema.COLUMNS` : base, table, colonne, type, nullabilité, index.
type ColumnRow = (String, String, String, String, String, String);

/// Lit tout le catalogue du serveur : bases, tables et colonnes, par nom.
pub async fn load(connection: &mut Connection) -> Result<Vec<SchemaInfo>, MysqlError> {
    let schemas: Vec<String> = connection
        .query("SELECT SCHEMA_NAME FROM information_schema.SCHEMATA")
        .await?;
    let tables: Vec<TableRow> = connection
        .query(
            "SELECT TABLE_SCHEMA, TABLE_NAME, TABLE_TYPE, TABLE_ROWS FROM information_schema.TABLES",
        )
        .await?;
    let columns: Vec<ColumnRow> = connection
        .query(
            "SELECT TABLE_SCHEMA, TABLE_NAME, COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_KEY
             FROM information_schema.COLUMNS ORDER BY ORDINAL_POSITION",
        )
        .await?;
    Ok(assemble(schemas, tables, columns))
}

/// Range les lignes lues dans `information_schema` en arbre : bases, puis
/// tables, puis colonnes. Bases et tables sont triées par nom ; les colonnes
/// gardent l'ordre reçu (ordre de déclaration).
fn assemble(
    schemas: Vec<String>,
    tables: Vec<TableRow>,
    columns: Vec<ColumnRow>,
) -> Vec<SchemaInfo> {
    let mut tree: BTreeMap<String, BTreeMap<String, TableInfo>> = schemas
        .into_iter()
        .map(|schema| (schema, BTreeMap::new()))
        .collect();
    for (schema, name, table_type, estimated_rows) in tables {
        let table = TableInfo {
            name: name.clone(),
            kind: table_kind(&table_type),
            estimated_rows,
            columns: Vec::new(),
        };
        tree.entry(schema).or_default().insert(name, table);
    }
    for (schema, table, name, column_type, is_nullable, key) in columns {
        if let Some(table) = tree
            .get_mut(&schema)
            .and_then(|tables| tables.get_mut(&table))
        {
            table.columns.push(ColumnInfo {
                name,
                column_type,
                nullable: is_nullable == "YES",
                key,
            });
        }
    }
    tree.into_iter()
        .map(|(name, tables)| SchemaInfo {
            name,
            tables: tables.into_values().collect(),
        })
        .collect()
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
    fn assembles_the_tree() {
        let schemas = vec!["vide".to_string(), "test".to_string()];
        let tables = vec![
            (
                "test".to_string(),
                "v".to_string(),
                "VIEW".to_string(),
                None,
            ),
            (
                "test".to_string(),
                "t".to_string(),
                "BASE TABLE".to_string(),
                Some(2),
            ),
        ];
        let column = |table: &str, name: &str| {
            (
                "test".to_string(),
                table.to_string(),
                name.to_string(),
                "int".to_string(),
                "NO".to_string(),
                String::new(),
            )
        };
        let columns = vec![column("t", "id"), column("v", "id"), column("t", "label")];

        let tree = assemble(schemas, tables, columns);

        let names: Vec<_> = tree.iter().map(|schema| schema.name.as_str()).collect();
        assert_eq!(names, ["test", "vide"]);
        let tables: Vec<_> = tree[0]
            .tables
            .iter()
            .map(|table| table.name.as_str())
            .collect();
        assert_eq!(tables, ["t", "v"]);
        let columns: Vec<_> = tree[0].tables[0]
            .columns
            .iter()
            .map(|column| column.name.as_str())
            .collect();
        assert_eq!(columns, ["id", "label"]);
        assert!(tree[1].tables.is_empty());
    }

    #[test]
    fn translates_table_types() {
        assert_eq!(table_kind("BASE TABLE"), TableKind::Table);
        assert_eq!(table_kind("SYSTEM VERSIONED"), TableKind::Table);
        assert_eq!(table_kind("VIEW"), TableKind::View);
        assert_eq!(table_kind("SYSTEM VIEW"), TableKind::System);
    }
}
