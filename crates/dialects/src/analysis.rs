//! Analyse d'une requête : le rôle de chaque mot, d'après l'arbre syntaxique
//! de `sqlparser`, le catalogue de la connexion et la version du serveur.
//!
//! C'est l'étape qui précède l'enjolivement (et, plus tard, la résolution
//! des noms du moteur fédéré) : la requête est comprise avant d'être
//! réécrite, au lieu d'être traitée mot à mot d'après des listes.
//!
//! - **Noms** : tout identifiant de l'arbre (base, table, colonne, alias,
//!   fonction...) est un nom, jamais un mot-clef, même s'il ressemble à un
//!   mot-clef (`cast`, `date`, `status`). Il prend l'écriture du catalogue
//!   quand le catalogue la connaît sans ambiguïté ; un alias prend celle de
//!   sa déclaration. Un nom inconnu, ambigu (`Clients` et `clients`) ou entre
//!   accents graves garde l'écriture tapée.
//! - **Mots-clefs** : tout autre mot que `sqlparser` reconnaît comme
//!   mot-clef (`SELECT`, `CAST`, `INTERVAL ... DAY`...). Un mot-clef que la
//!   version du serveur ne connaît pas (fichiers `keywords/*.yaml`, champs
//!   `since` / `until`) est signalé à part.
//!
//! Les identifiants sont relevés dans l'arbre sérialisé (option `serde` de
//! `sqlparser`), où chacun porte sa position : aucun n'échappe au relevé,
//! où qu'il soit dans la requête. Leur nature (table, colonne...) vient d'un
//! parcours de l'arbre ; un nom de nature inconnue garde son écriture.

use std::collections::HashMap;
use std::ops::ControlFlow;

use serde_json::Value;
use sqlparser::ast::{
    Expr, ObjectName, ObjectNamePart, Query, SelectItem, SetExpr, TableFactor, Visit, Visitor,
};
use sqlparser::parser::Parser;
use sqlparser::tokenizer::{Token, Tokenizer};

use crate::Dialect;
use crate::keywords::is_known_in_version;
use crate::limit::byte_offset;

/// Noms du catalogue de la connexion, tels que définis (avec leur casse).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalogNames {
    /// Bases.
    pub schemas: Vec<String>,
    /// Tables et vues, toutes bases confondues.
    pub tables: Vec<String>,
    /// Colonnes, toutes tables confondues.
    pub columns: Vec<String>,
    /// Fonctions et procédures stockées.
    pub routines: Vec<String>,
}

/// Rôle d'un mot de la requête.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    /// Mot-clef du langage, connu de la version du serveur.
    Keyword,
    /// Mot-clef pour l'analyseur, mais inconnu de la version du serveur.
    UnknownKeyword,
    /// Nom (base, table, colonne, alias, fonction...), avec l'écriture à lui donner.
    Name {
        /// Écriture du nom : celle du catalogue, de la déclaration de
        /// l'alias, ou celle tapée.
        spelling: String,
    },
}

/// Un mot de la requête et son rôle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    /// Début du mot dans le texte, en octets.
    pub start: usize,
    /// Fin du mot dans le texte, en octets.
    pub end: usize,
    /// Rôle du mot.
    pub role: Role,
}

/// Nature d'un nom, d'après sa place dans l'arbre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Base, devant un nom de table ou de colonne.
    Schema,
    /// Table ou vue, ou alias de table employé devant une colonne.
    Table,
    /// Colonne, ou alias de colonne employé (`ORDER BY total`).
    Column,
    /// Déclaration d'un alias de table (`FROM t AS a`, nom d'une CTE).
    TableAlias,
    /// Déclaration d'un alias de colonne (`SELECT x AS total`).
    ColumnAlias,
    /// Fonction ou procédure (son écriture est réglée par le module `functions`).
    Function,
}

/// Position d'un identifiant dans le texte : ligne et colonne de son début.
type Position = (u64, u64);

/// Analyse un texte (une ou plusieurs requêtes). `None` si `sqlparser` ne
/// le comprend pas : l'appelant se rabat alors sur un traitement mot à mot.
///
/// `version` est celle annoncée par le serveur (`10.11.11-MariaDB`) ; sans
/// version, tous les mots-clefs du dialecte sont connus.
pub fn analyze(
    sql: &str,
    dialect: Dialect,
    version: Option<&str>,
    catalog: &CatalogNames,
) -> Option<Vec<Word>> {
    let parser_dialect = dialect.parser_dialect();
    let statements = Parser::parse_sql(parser_dialect.as_ref(), sql).ok()?;

    let identifiers = identifier_positions(&serde_json::to_value(&statements).ok()?);
    let mut kinds = KindCollector::default();
    let _ = statements.visit(&mut kinds);
    let names = Names::new(catalog, &kinds);

    let tokens = Tokenizer::new(parser_dialect.as_ref(), sql)
        .tokenize_with_location()
        .ok()?;
    let mut words = Vec::new();
    for token in tokens {
        let Token::Word(word) = &token.token else {
            continue;
        };
        if word.quote_style.is_some() {
            continue;
        }
        let position = (token.span.start.line, token.span.start.column);
        let role = if identifiers.contains_key(&position) {
            let kind = kinds.kinds.get(&position).copied();
            Role::Name {
                spelling: names.spelling(kind, &word.value),
            }
        } else if word.keyword != sqlparser::keywords::Keyword::NoKeyword {
            if is_known_in_version(&word.value, dialect, version) {
                Role::Keyword
            } else {
                Role::UnknownKeyword
            }
        } else {
            continue;
        };
        let (Some(start), Some(end)) = (
            byte_offset(sql, token.span.start),
            byte_offset(sql, token.span.end),
        ) else {
            return None;
        };
        words.push(Word { start, end, role });
    }
    Some(words)
}

/// Positions de tous les identifiants de l'arbre sérialisé : un identifiant
/// y est un objet `{ value, quote_style, span }`.
fn identifier_positions(tree: &Value) -> HashMap<Position, bool> {
    let mut positions = HashMap::new();
    collect_identifiers(tree, &mut positions);
    positions
}

/// Parcourt l'arbre sérialisé et note la position de chaque identifiant,
/// avec un indicateur « entre guillemets ».
fn collect_identifiers(node: &Value, positions: &mut HashMap<Position, bool>) {
    match node {
        Value::Object(fields) => {
            let is_identifier = fields.len() == 3
                && fields.get("value").is_some_and(Value::is_string)
                && fields.contains_key("quote_style")
                && fields.contains_key("span");
            if is_identifier {
                let start = &fields["span"]["start"];
                if let (Some(line), Some(column)) =
                    (start["line"].as_u64(), start["column"].as_u64())
                    && line > 0
                {
                    positions.insert((line, column), !fields["quote_style"].is_null());
                }
            }
            fields
                .values()
                .for_each(|value| collect_identifiers(value, positions));
        }
        Value::Array(items) => items
            .iter()
            .for_each(|item| collect_identifiers(item, positions)),
        _ => {}
    }
}

/// Parcours de l'arbre qui note la nature de chaque nom, et les alias déclarés.
#[derive(Default)]
struct KindCollector {
    /// Nature de chaque nom, par position.
    kinds: HashMap<Position, Kind>,
    /// Alias de table déclarés, avec leur écriture.
    table_aliases: Vec<String>,
    /// Alias de colonne déclarés, avec leur écriture.
    column_aliases: Vec<String>,
}

impl KindCollector {
    /// Note la nature d'un identifiant.
    fn note(&mut self, ident: &sqlparser::ast::Ident, kind: Kind) {
        let start = ident.span.start;
        if start.line > 0 {
            self.kinds.insert((start.line, start.column), kind);
        }
    }

    /// Note les parties d'un nom qualifié : la dernière a la nature donnée,
    /// celles qui la précèdent sont, de droite à gauche, `qualifiers`.
    fn note_parts(&mut self, parts: &[&sqlparser::ast::Ident], last: Kind, qualifiers: &[Kind]) {
        let mut remaining = parts.iter().rev();
        if let Some(ident) = remaining.next() {
            self.note(ident, last);
        }
        for (ident, kind) in remaining.zip(qualifiers) {
            self.note(ident, *kind);
        }
    }

    /// Note les alias de colonne d'un corps de requête (`SELECT x AS total`),
    /// opérations ensemblistes comprises.
    fn note_projection(&mut self, body: &SetExpr) {
        match body {
            SetExpr::Select(select) => {
                for item in &select.projection {
                    if let SelectItem::ExprWithAlias { alias, .. } = item {
                        self.note(alias, Kind::ColumnAlias);
                        self.column_aliases.push(alias.value.clone());
                    }
                }
            }
            SetExpr::SetOperation { left, right, .. } => {
                self.note_projection(left);
                self.note_projection(right);
            }
            SetExpr::Query(query) => self.note_projection(&query.body),
            _ => {}
        }
    }
}

/// Identifiants d'un nom d'objet (`base.table`), sans les parties calculées.
fn object_idents(name: &ObjectName) -> Vec<&sqlparser::ast::Ident> {
    name.0.iter().filter_map(ObjectNamePart::as_ident).collect()
}

impl Visitor for KindCollector {
    type Break = ();

    fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<Self::Break> {
        if let Some(with) = &query.with {
            for cte in &with.cte_tables {
                self.note(&cte.alias.name, Kind::TableAlias);
                self.table_aliases.push(cte.alias.name.value.clone());
            }
        }
        self.note_projection(&query.body);
        ControlFlow::Continue(())
    }

    fn pre_visit_relation(&mut self, relation: &ObjectName) -> ControlFlow<Self::Break> {
        self.note_parts(&object_idents(relation), Kind::Table, &[Kind::Schema]);
        ControlFlow::Continue(())
    }

    fn pre_visit_table_factor(&mut self, factor: &TableFactor) -> ControlFlow<Self::Break> {
        if let TableFactor::Table {
            alias: Some(alias), ..
        }
        | TableFactor::Derived {
            alias: Some(alias), ..
        } = factor
        {
            self.note(&alias.name, Kind::TableAlias);
            self.table_aliases.push(alias.name.value.clone());
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<Self::Break> {
        match expr {
            Expr::Identifier(ident) => self.note(ident, Kind::Column),
            Expr::CompoundIdentifier(parts) => {
                let parts: Vec<_> = parts.iter().collect();
                self.note_parts(&parts, Kind::Column, &[Kind::Table, Kind::Schema]);
            }
            Expr::Function(function) => {
                self.note_parts(
                    &object_idents(&function.name),
                    Kind::Function,
                    &[Kind::Schema],
                );
            }
            _ => {}
        }
        ControlFlow::Continue(())
    }
}

/// Écriture des noms : catalogue et alias déclarés, comparés sans tenir
/// compte de la casse.
struct Names<'a> {
    /// Noms du catalogue.
    catalog: &'a CatalogNames,
    /// Alias de table déclarés dans la requête.
    table_aliases: Vec<String>,
    /// Alias de colonne déclarés dans la requête.
    column_aliases: Vec<String>,
}

impl<'a> Names<'a> {
    /// Prépare l'écriture des noms d'une requête.
    fn new(catalog: &'a CatalogNames, kinds: &KindCollector) -> Self {
        Self {
            catalog,
            table_aliases: kinds.table_aliases.clone(),
            column_aliases: kinds.column_aliases.clone(),
        }
    }

    /// Écriture d'un nom tapé `typed`, de nature `kind` (inconnue : `None`).
    fn spelling(&self, kind: Option<Kind>, typed: &str) -> String {
        let found =
            match kind {
                Some(Kind::Schema) => unique(&self.catalog.schemas, typed),
                Some(Kind::Table) => unique(&self.table_aliases, typed)
                    .or_else(|| unique(&self.catalog.tables, typed)),
                Some(Kind::Column) => unique(&self.column_aliases, typed)
                    .or_else(|| unique(&self.catalog.columns, typed)),
                Some(Kind::TableAlias | Kind::ColumnAlias | Kind::Function) | None => None,
            };
        found.unwrap_or_else(|| typed.to_string())
    }
}

/// L'écriture d'un nom dans `names`, s'il y figure sous une seule écriture
/// (sans tenir compte de la casse) ; `None` s'il n'y figure pas ou sous
/// plusieurs écritures (`Clients` et `clients`).
fn unique(names: &[String], typed: &str) -> Option<String> {
    let mut matches = names.iter().filter(|name| name.eq_ignore_ascii_case(typed));
    let first = matches.next()?;
    matches.all(|other| other == first).then(|| first.clone())
}

#[cfg(test)]
mod tests {
    use sqlparser::tokenizer::Location;

    use super::*;

    /// Ligne et colonne d'un emplacement.
    fn position_of(location: Location) -> Position {
        (location.line, location.column)
    }

    /// Catalogue de test : une base, deux tables, leurs colonnes, une fonction.
    fn catalog() -> CatalogNames {
        CatalogNames {
            schemas: vec!["Compta".to_string()],
            tables: vec![
                "operations".to_string(),
                "Clients".to_string(),
                "clients".to_string(),
            ],
            columns: vec![
                "operation_id".to_string(),
                "cast".to_string(),
                "Montant".to_string(),
            ],
            routines: vec!["TotalTtc".to_string()],
        }
    }

    /// Réécrit le texte d'après l'analyse : mots-clefs en capitales, noms
    /// dans leur écriture, mots-clefs inconnus de la version entre crochets.
    fn rewrite(sql: &str, dialect: Dialect, version: Option<&str>) -> String {
        let words = analyze(sql, dialect, version, &catalog()).unwrap();
        let mut result = String::new();
        let mut last = 0;
        for word in words {
            result.push_str(&sql[last..word.start]);
            let text = &sql[word.start..word.end];
            match word.role {
                Role::Keyword => result.push_str(&text.to_ascii_uppercase()),
                Role::UnknownKeyword => result.push_str(&format!("[{text}]")),
                Role::Name { spelling } => result.push_str(&spelling),
            }
            last = word.end;
        }
        result.push_str(&sql[last..]);
        result
    }

    #[test]
    fn tells_keywords_from_names_that_look_like_keywords() {
        assert_eq!(
            rewrite(
                "select cast(cast as char), date '2026-10-14' + interval 1 day from t",
                Dialect::MySql,
                None
            ),
            "SELECT CAST(cast AS CHAR), DATE '2026-10-14' + INTERVAL 1 DAY FROM t"
        );
    }

    #[test]
    fn spells_names_as_the_catalog_does() {
        assert_eq!(
            rewrite(
                "select OPERATION_ID, montant from COMPTA.Operations where operation_id > 0",
                Dialect::MySql,
                None
            ),
            "SELECT operation_id, Montant FROM Compta.operations WHERE operation_id > 0"
        );
    }

    #[test]
    fn keeps_unknown_ambiguous_and_quoted_names_as_typed() {
        assert_eq!(
            rewrite(
                "select x from CLIENTS join `OPERATIONS` join Inconnue",
                Dialect::MySql,
                None
            ),
            "SELECT x FROM CLIENTS JOIN `OPERATIONS` JOIN Inconnue"
        );
    }

    #[test]
    fn spells_aliases_as_declared() {
        assert_eq!(
            rewrite(
                "select O.operation_id total from operations Op_1 order by TOTAL",
                Dialect::MySql,
                None
            ),
            "SELECT O.operation_id total FROM operations Op_1 ORDER BY total"
        );
        assert_eq!(
            rewrite(
                "select OP_1.operation_id from operations Op_1",
                Dialect::MySql,
                None
            ),
            "SELECT Op_1.operation_id FROM operations Op_1"
        );
    }

    #[test]
    fn flags_keywords_unknown_to_the_server_version() {
        let sql = "with recursive r as (select 1) select * from r";
        assert_eq!(
            rewrite(sql, Dialect::MySql, Some("5.7.44")),
            "WITH [recursive] r AS (SELECT 1) SELECT * FROM r"
        );
        assert_eq!(
            rewrite(sql, Dialect::MySql, Some("8.0.36")),
            "WITH RECURSIVE r AS (SELECT 1) SELECT * FROM r"
        );
    }

    #[test]
    fn gives_up_on_unparsable_text() {
        assert_eq!(
            analyze("select from where", Dialect::MySql, None, &catalog()),
            None
        );
    }

    #[test]
    fn records_identifier_positions() {
        let sql = "select a from t";
        let statements = Parser::parse_sql(Dialect::MySql.parser_dialect().as_ref(), sql).unwrap();
        let positions = identifier_positions(&serde_json::to_value(&statements).unwrap());
        assert!(positions.contains_key(&position_of(Location::new(1, 8))));
        assert!(positions.contains_key(&position_of(Location::new(1, 15))));
    }
}
