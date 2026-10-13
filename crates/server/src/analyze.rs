//! Analyse du texte de l'éditeur, demandée par l'interface peu après chaque
//! frappe : découpage en requêtes, pour mettre en évidence celle que
//! Ctrl-Entrée exécuterait, et erreurs de syntaxe, pour les souligner.
//!
//! Le découpage est celui des sessions de requêtes (crate `splitter`) :
//! l'interface montre exactement la requête qui partira. Chaque requête est
//! ensuite analysée seule (`prisme_dialects::syntax`). Les positions sont en
//! unités UTF-16, comme celles de l'éditeur.

use prisme_connectors::mysql::Flavor;
use prisme_dialects::Dialect;
use prisme_dialects::syntax::syntax_error;
use prisme_splitter::{Statement, split};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Texte à analyser.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct AnalyzeRequest {
    /// Texte complet de l'éditeur.
    pub sql: String,
    /// Variante du serveur de la session, qui fixe le dialecte d'analyse.
    pub flavor: Flavor,
}

/// Résultat de l'analyse.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct Analysis {
    /// Requêtes du texte, dans l'ordre.
    pub statements: Vec<StatementSpan>,
    /// Erreurs de syntaxe, au plus une par requête.
    pub errors: Vec<SyntaxIssue>,
}

/// Erreur de syntaxe à souligner dans l'éditeur.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct SyntaxIssue {
    /// Début du passage souligné, en unités UTF-16.
    pub start: usize,
    /// Fin du passage souligné, en unités UTF-16.
    pub end: usize,
    /// Message affiché au survol.
    pub message: String,
}

/// Emplacement d'une requête dans le texte, en unités UTF-16.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct StatementSpan {
    /// Premier caractère significatif de la requête.
    pub start: usize,
    /// Fin de la requête, séparateur compris.
    pub end: usize,
}

/// Analyse le texte de l'éditeur.
pub fn analyze(request: &AnalyzeRequest) -> Analysis {
    let dialect = match request.flavor {
        Flavor::MySql => Dialect::MySql,
        Flavor::MariaDb => Dialect::MariaDb,
    };
    let statements = split(&request.sql);
    Analysis {
        errors: statements
            .iter()
            .filter_map(|statement| issue(statement, dialect))
            .collect(),
        statements: statements
            .iter()
            .map(|statement| StatementSpan {
                start: statement.start,
                end: statement.end,
            })
            .collect(),
    }
}

/// Erreur de syntaxe d'une requête, placée dans le texte de l'éditeur.
///
/// Le passage souligné est le mot qui commence à la position de l'erreur ;
/// sans position connue (fin de requête inattendue), le dernier caractère de
/// la requête.
fn issue(statement: &Statement, dialect: Dialect) -> Option<SyntaxIssue> {
    let error = syntax_error(&statement.sql, dialect)?;
    let (start, end) = locate(&statement.sql, error.line, error.column)
        .unwrap_or_else(|| last_character(&statement.sql));
    Some(SyntaxIssue {
        start: statement.start + start,
        end: statement.start + end,
        message: format!("Requête non comprise par Prisme : {}", error.message),
    })
}

/// Passage du mot qui commence ligne `line`, colonne `column` (en
/// caractères, à partir de 1) : début et fin en unités UTF-16, depuis le
/// début de `sql`. `None` si la position est inconnue ou hors du texte.
fn locate(sql: &str, line: usize, column: usize) -> Option<(usize, usize)> {
    let (mut current_line, mut current_column, mut offset) = (1, 1, 0);
    let mut chars = sql.chars().peekable();
    while let Some(&character) = chars.peek() {
        if (current_line, current_column) == (line, column) {
            let word: usize = chars
                .take_while(|character| !character.is_whitespace())
                .map(char::len_utf16)
                .sum();
            return Some((offset, offset + word.max(1)));
        }
        if character == '\n' {
            (current_line, current_column) = (current_line + 1, 1);
        } else {
            current_column += 1;
        }
        offset += character.len_utf16();
        chars.next();
    }
    None
}

/// Passage du dernier caractère de `sql`, en unités UTF-16 (vide si `sql` est vide).
fn last_character(sql: &str) -> (usize, usize) {
    let end: usize = sql.encode_utf16().count();
    let width = sql.chars().last().map_or(0, char::len_utf16);
    (end - width, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gives_the_place_of_each_statement() {
        let request = AnalyzeRequest {
            sql: "SELECT 1;\n-- é\nSELECT 2".to_string(),
            flavor: Flavor::MariaDb,
        };
        assert_eq!(
            analyze(&request).statements,
            vec![
                StatementSpan { start: 0, end: 9 },
                StatementSpan { start: 15, end: 23 },
            ]
        );
    }

    #[test]
    fn underlines_the_word_at_the_error() {
        // « é » compte pour une unité UTF-16, « 😀 » pour deux.
        let request = AnalyzeRequest {
            sql: "SELECT 'é';\nSELECT '😀'\nFROM t WHERE a = = 1;".to_string(),
            flavor: Flavor::MySql,
        };
        let errors = analyze(&request).errors;
        assert_eq!(errors.len(), 1);
        let start = request.sql.find("= 1").unwrap();
        let start = request.sql[..start].encode_utf16().count();
        assert_eq!((errors[0].start, errors[0].end), (start, start + 1));
        assert!(
            errors[0]
                .message
                .starts_with("Requête non comprise par Prisme")
        );
    }

    #[test]
    fn underlines_the_last_character_of_an_unfinished_statement() {
        let request = AnalyzeRequest {
            sql: "SELECT * FROM t WHERE".to_string(),
            flavor: Flavor::MySql,
        };
        let errors = analyze(&request).errors;
        assert_eq!((errors[0].start, errors[0].end), (20, 21));
    }
}
