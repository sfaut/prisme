//! Dialectes SQL de Prisme : analyse et réécriture des requêtes selon le dialecte.
//!
//! S'appuie sur `sqlparser` (voir `docs/V1.md`, section 3.2). Pour l'instant :
//! la limite de lignes par défaut (module `limit`), la classification
//! lecture / écriture (module `classify`), l'enjoliveur (module `beautify`)
//! et la position des erreurs de syntaxe (module `syntax`).
//! Le catalogue de fonctions et les émetteurs par dialecte viendront avec la
//! traduction.

mod alias;
pub mod beautify;
pub mod classify;
mod functions;
mod keywords;
mod layout;
pub mod limit;
mod parse;
pub mod syntax;
mod whitespace;

/// Dialecte SQL d'une requête.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// MySQL.
    MySql,
    /// MariaDB. `sqlparser` n'a pas de dialecte MariaDB : celui de MySQL sert de base.
    MariaDb,
}

impl Dialect {
    /// Dialecte `sqlparser` utilisé pour analyser une requête de ce dialecte.
    fn parser_dialect(self) -> Box<dyn sqlparser::dialect::Dialect> {
        match self {
            Self::MySql | Self::MariaDb => Box::new(sqlparser::dialect::MySqlDialect {}),
        }
    }
}
