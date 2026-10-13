//! Catalogue d'un serveur MySQL ou MariaDB : bases, objets et détail des tables.
//!
//! Sert à l'explorateur d'objets, qui affiche les éléments dès qu'ils
//! arrivent, en trois temps :
//!
//! 1. `schemas` : noms des bases ;
//! 2. `objects` : tables, vues, procédures et fonctions d'une base ;
//! 3. `details` : colonnes, index, clefs étrangères et déclencheurs de chaque
//!    table d'une base.
//!
//! Chaque lecture filtre sur une base, ce qui évite au serveur de parcourir
//! toutes les tables à la fois. Les requêtes ne demandent que des
//! informations du dictionnaire : demander aussi le nombre de lignes
//! (`TABLE_ROWS`) obligerait le serveur à ouvrir chaque table, ce qui est très
//! lent sur un gros serveur. Les noms de base sont passés en paramètres de
//! requêtes préparées, jamais concaténés au texte SQL.

use std::collections::BTreeMap;

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

/// Procédure ou fonction stockée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct RoutineInfo {
    /// Nom de la routine.
    pub name: String,
    /// Procédure ou fonction.
    pub kind: RoutineKind,
}

/// Nature d'une routine stockée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum RoutineKind {
    /// Procédure (`CALL`).
    Procedure,
    /// Fonction (appelée dans une expression).
    Function,
}

/// Objets d'une base : tables et vues, procédures et fonctions, par nom.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct SchemaObjects {
    /// Tables et vues.
    pub tables: Vec<TableInfo>,
    /// Procédures et fonctions.
    pub routines: Vec<RoutineInfo>,
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
    /// Valeur attribuée automatiquement à l'insertion (`AUTO_INCREMENT`) ?
    pub auto_increment: bool,
}

/// Index d'une table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct IndexInfo {
    /// Nom de l'index (`PRIMARY` pour la clef primaire).
    pub name: String,
    /// Colonnes indexées, dans l'ordre de l'index. Une partie calculée
    /// (index fonctionnel de MySQL) apparaît sous la forme `(expression)`.
    pub columns: Vec<String>,
    /// Index unique ?
    pub unique: bool,
}

/// Clef étrangère d'une table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ForeignKeyInfo {
    /// Nom de la contrainte.
    pub name: String,
    /// Colonnes de la table, dans l'ordre de la clef.
    pub columns: Vec<String>,
    /// Base de la table référencée.
    pub referenced_schema: String,
    /// Table référencée.
    pub referenced_table: String,
    /// Colonnes référencées, dans le même ordre que `columns`.
    pub referenced_columns: Vec<String>,
}

/// Déclencheur d'une table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TriggerInfo {
    /// Nom du déclencheur.
    pub name: String,
    /// Moment : `BEFORE` ou `AFTER`.
    pub timing: String,
    /// Événement : `INSERT`, `UPDATE` ou `DELETE`.
    pub event: String,
}

/// Détail d'une table : colonnes, index, clefs étrangères et déclencheurs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TableDetails {
    /// Nom de la table.
    pub table: String,
    /// Colonnes, dans l'ordre de déclaration.
    pub columns: Vec<ColumnInfo>,
    /// Index, la clef primaire en premier, puis par nom.
    pub indexes: Vec<IndexInfo>,
    /// Clefs étrangères, par nom.
    pub foreign_keys: Vec<ForeignKeyInfo>,
    /// Déclencheurs, par moment, événement et ordre d'exécution.
    pub triggers: Vec<TriggerInfo>,
}

/// Ligne d'`information_schema.COLUMNS` : table, colonne, type, nullabilité,
/// index, informations complémentaires (`auto_increment`...).
type ColumnRow = (String, String, String, String, String, String);

/// Ligne d'`information_schema.STATISTICS` : table, index, non unique, colonne.
type IndexRow = (String, String, i64, Option<String>);

/// Ligne d'`information_schema.KEY_COLUMN_USAGE` : table, contrainte, colonne,
/// base, table et colonne référencées.
type ForeignKeyRow = (String, String, String, String, String, String);

/// Ligne d'`information_schema.TRIGGERS` : table, déclencheur, moment, événement.
type TriggerRow = (String, String, String, String);

/// Liste les bases du serveur, par nom.
pub async fn schemas(connection: &mut Connection) -> Result<Vec<String>, MysqlError> {
    Ok(connection
        .query("SELECT SCHEMA_NAME FROM information_schema.SCHEMATA ORDER BY SCHEMA_NAME")
        .await?)
}

/// Liste les objets de la base `schema` : tables, vues, procédures et fonctions.
pub async fn objects(
    connection: &mut Connection,
    schema: &str,
) -> Result<SchemaObjects, MysqlError> {
    let tables: Vec<(String, String)> = connection
        .exec(
            "SELECT TABLE_NAME, TABLE_TYPE FROM information_schema.TABLES
             WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME",
            (schema,),
        )
        .await?;
    let routines: Vec<(String, String)> = connection
        .exec(
            "SELECT ROUTINE_NAME, ROUTINE_TYPE FROM information_schema.ROUTINES
             WHERE ROUTINE_SCHEMA = ? ORDER BY ROUTINE_NAME",
            (schema,),
        )
        .await?;
    Ok(SchemaObjects {
        tables: tables
            .into_iter()
            .map(|(name, table_type)| TableInfo {
                name,
                kind: table_kind(&table_type),
            })
            .collect(),
        routines: routines
            .into_iter()
            .map(|(name, routine_type)| RoutineInfo {
                name,
                kind: if routine_type == "FUNCTION" {
                    RoutineKind::Function
                } else {
                    RoutineKind::Procedure
                },
            })
            .collect(),
    })
}

/// Lit le détail de toutes les tables de la base `schema`, table par table.
pub async fn details(
    connection: &mut Connection,
    schema: &str,
) -> Result<Vec<TableDetails>, MysqlError> {
    let columns: Vec<ColumnRow> = connection
        .exec(
            "SELECT TABLE_NAME, COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_KEY, EXTRA
             FROM information_schema.COLUMNS
             WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME, ORDINAL_POSITION",
            (schema,),
        )
        .await?;
    let indexes: Vec<IndexRow> = connection
        .exec(
            "SELECT TABLE_NAME, INDEX_NAME, NON_UNIQUE, COLUMN_NAME
             FROM information_schema.STATISTICS
             WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME, INDEX_NAME, SEQ_IN_INDEX",
            (schema,),
        )
        .await?;
    let foreign_keys: Vec<ForeignKeyRow> = connection
        .exec(
            "SELECT TABLE_NAME, CONSTRAINT_NAME, COLUMN_NAME,
                    REFERENCED_TABLE_SCHEMA, REFERENCED_TABLE_NAME, REFERENCED_COLUMN_NAME
             FROM information_schema.KEY_COLUMN_USAGE
             WHERE TABLE_SCHEMA = ? AND REFERENCED_TABLE_NAME IS NOT NULL
             ORDER BY TABLE_NAME, CONSTRAINT_NAME, ORDINAL_POSITION",
            (schema,),
        )
        .await?;
    let triggers: Vec<TriggerRow> = connection
        .exec(
            "SELECT EVENT_OBJECT_TABLE, TRIGGER_NAME, ACTION_TIMING, EVENT_MANIPULATION
             FROM information_schema.TRIGGERS
             WHERE TRIGGER_SCHEMA = ?
             ORDER BY EVENT_OBJECT_TABLE, ACTION_TIMING, EVENT_MANIPULATION, ACTION_ORDER",
            (schema,),
        )
        .await?;
    Ok(assemble_details(columns, indexes, foreign_keys, triggers))
}

/// Range les lignes lues dans `information_schema` table par table.
///
/// Les lignes arrivent triées par table puis dans l'ordre utile (position
/// des colonnes, ordre des colonnes dans l'index ou la clef). Le résultat
/// est trié par nom de table.
fn assemble_details(
    columns: Vec<ColumnRow>,
    indexes: Vec<IndexRow>,
    foreign_keys: Vec<ForeignKeyRow>,
    triggers: Vec<TriggerRow>,
) -> Vec<TableDetails> {
    let mut tables: BTreeMap<String, TableDetails> = BTreeMap::new();
    for (name, column, column_type, is_nullable, key, extra) in columns {
        table(&mut tables, &name).columns.push(ColumnInfo {
            name: column,
            column_type,
            nullable: is_nullable == "YES",
            key,
            // EXTRA peut cumuler plusieurs mentions (« auto_increment », « on update... »).
            auto_increment: extra.to_lowercase().contains("auto_increment"),
        });
    }
    for (name, index, non_unique, column) in indexes {
        let column = column.unwrap_or_else(|| "(expression)".to_string());
        let indexes = &mut table(&mut tables, &name).indexes;
        match indexes.last_mut() {
            Some(last) if last.name == index => last.columns.push(column),
            _ => indexes.push(IndexInfo {
                name: index,
                columns: vec![column],
                unique: non_unique == 0,
            }),
        }
    }
    for (name, constraint, column, referenced_schema, referenced_table, referenced_column) in
        foreign_keys
    {
        let keys = &mut table(&mut tables, &name).foreign_keys;
        match keys.last_mut() {
            Some(last) if last.name == constraint => {
                last.columns.push(column);
                last.referenced_columns.push(referenced_column);
            }
            _ => keys.push(ForeignKeyInfo {
                name: constraint,
                columns: vec![column],
                referenced_schema,
                referenced_table,
                referenced_columns: vec![referenced_column],
            }),
        }
    }
    for (name, trigger, timing, event) in triggers {
        table(&mut tables, &name).triggers.push(TriggerInfo {
            name: trigger,
            timing,
            event,
        });
    }

    let mut tables: Vec<TableDetails> = tables.into_values().collect();
    for details in &mut tables {
        // Clef primaire en premier, comme dans SHOW INDEX ; le tri est stable.
        details.indexes.sort_by_key(|index| index.name != "PRIMARY");
    }
    tables
}

/// Renvoie le détail de la table `name`, créé vide s'il n'existe pas encore.
fn table<'a>(tables: &'a mut BTreeMap<String, TableDetails>, name: &str) -> &'a mut TableDetails {
    tables
        .entry(name.to_string())
        .or_insert_with(|| TableDetails {
            table: name.to_string(),
            ..TableDetails::default()
        })
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

    fn text(value: &str) -> String {
        value.to_string()
    }

    #[test]
    fn assembles_details_table_by_table() {
        let columns = vec![
            (
                text("t"),
                text("id"),
                text("int"),
                text("NO"),
                text("PRI"),
                text("auto_increment"),
            ),
            (
                text("t"),
                text("parent"),
                text("int"),
                text("YES"),
                text("MUL"),
                text(""),
            ),
            (
                text("u"),
                text("id"),
                text("int"),
                text("NO"),
                text("PRI"),
                text(""),
            ),
        ];
        let indexes = vec![
            (text("t"), text("idx_parent"), 1, Some(text("parent"))),
            (text("t"), text("PRIMARY"), 0, Some(text("id"))),
            (text("t"), text("uq_pair"), 0, Some(text("id"))),
            (text("t"), text("uq_pair"), 0, Some(text("parent"))),
            (text("u"), text("idx_expr"), 1, None),
        ];
        let foreign_keys = vec![(
            text("t"),
            text("fk_parent"),
            text("parent"),
            text("shop"),
            text("u"),
            text("id"),
        )];
        let triggers = vec![(text("t"), text("t_bi"), text("BEFORE"), text("INSERT"))];

        let tables = assemble_details(columns, indexes, foreign_keys, triggers);

        assert_eq!(tables.len(), 2);
        let t = &tables[0];
        assert_eq!(t.table, "t");
        assert_eq!(t.columns.len(), 2);
        assert!(t.columns[0].auto_increment);
        assert!(!t.columns[1].auto_increment);
        let names: Vec<_> = t.indexes.iter().map(|index| index.name.as_str()).collect();
        assert_eq!(names, ["PRIMARY", "idx_parent", "uq_pair"]);
        assert_eq!(t.indexes[2].columns, ["id", "parent"]);
        assert!(t.indexes[2].unique);
        assert!(!t.indexes[1].unique);
        assert_eq!(t.foreign_keys[0].referenced_table, "u");
        assert_eq!(t.triggers[0].name, "t_bi");
        assert_eq!(tables[1].indexes[0].columns, ["(expression)"]);
    }

    #[test]
    fn translates_table_types() {
        assert_eq!(table_kind("BASE TABLE"), TableKind::Table);
        assert_eq!(table_kind("SYSTEM VERSIONED"), TableKind::Table);
        assert_eq!(table_kind("VIEW"), TableKind::View);
        assert_eq!(table_kind("SYSTEM VIEW"), TableKind::System);
    }
}
