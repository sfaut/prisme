//! Erreur de syntaxe d'une requête, avec sa position, pour la souligner dans
//! l'éditeur avant toute exécution.
//!
//! L'erreur est celle de `sqlparser`, dont le message se termine par sa
//! position (« at Line: 2, Column: 8 ») : elle en est extraite. Une requête
//! valide pour le serveur peut être refusée par `sqlparser` (corps de
//! procédure, syntaxe propre à une version) : l'erreur dit alors que Prisme
//! ne la comprend pas, ce qui reste vrai, puisque la session refuse les
//! requêtes non analysables.

use sqlparser::parser::{Parser, ParserError};

use crate::Dialect;

/// Erreur de syntaxe d'une requête.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    /// Message de `sqlparser`, sans la position.
    pub message: String,
    /// Ligne de l'erreur dans la requête, à partir de 1 ; 0 si inconnue
    /// (erreur en fin de requête, par exemple).
    pub line: usize,
    /// Colonne de l'erreur, en caractères, à partir de 1 ; 0 si inconnue.
    pub column: usize,
}

/// Marque de la position à la fin d'un message de `sqlparser`.
const LOCATION_MARK: &str = " at Line: ";

/// Analyse une requête seule et renvoie son erreur de syntaxe, `None` si elle est comprise.
pub fn syntax_error(sql: &str, dialect: Dialect) -> Option<SyntaxError> {
    let error = Parser::parse_sql(dialect.parser_dialect().as_ref(), sql).err()?;
    let text = match error {
        ParserError::ParserError(text) | ParserError::TokenizerError(text) => text,
        ParserError::RecursionLimitExceeded => "requête trop imbriquée".to_string(),
    };
    Some(with_location(&text))
}

/// Sépare un message de `sqlparser` de sa position finale, s'il en a une.
fn with_location(text: &str) -> SyntaxError {
    let parsed = text
        .rsplit_once(LOCATION_MARK)
        .and_then(|(message, location)| {
            let (line, column) = location.split_once(", Column: ")?;
            Some(SyntaxError {
                message: message.to_string(),
                line: line.parse().ok()?,
                column: column.trim().parse().ok()?,
            })
        });
    parsed.unwrap_or_else(|| SyntaxError {
        message: text.to_string(),
        line: 0,
        column: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locates_the_error() {
        let error = syntax_error("SELECT 1\nFROM t WHERE", Dialect::MySql).unwrap();
        assert_eq!((error.line, error.column), (0, 0));

        let error = syntax_error("SELECT *\nFROM t WHERE a = = 1", Dialect::MySql).unwrap();
        assert_eq!((error.line, error.column), (2, 18));
        assert!(!error.message.contains("Line"));

        let error = syntax_error("SELEC 1", Dialect::MariaDb).unwrap();
        assert_eq!((error.line, error.column), (1, 1));
    }

    #[test]
    fn accepts_a_valid_statement() {
        assert_eq!(
            syntax_error("SELECT * FROM t WHERE a = 1", Dialect::MySql),
            None
        );
    }
}
