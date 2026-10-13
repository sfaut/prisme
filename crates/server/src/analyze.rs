//! Analyse du texte de l'éditeur, demandée par l'interface peu après chaque
//! frappe : découpage en requêtes, pour mettre en évidence celle que
//! Ctrl-Entrée exécuterait.
//!
//! Le découpage est celui des sessions de requêtes (crate `splitter`) :
//! l'interface montre exactement la requête qui partira. Les positions sont en
//! unités UTF-16, comme celles de l'éditeur.

use prisme_splitter::split;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Texte à analyser.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct AnalyzeRequest {
    /// Texte complet de l'éditeur.
    pub sql: String,
}

/// Résultat de l'analyse.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct Analysis {
    /// Requêtes du texte, dans l'ordre.
    pub statements: Vec<StatementSpan>,
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
    let statements = split(&request.sql)
        .into_iter()
        .map(|statement| StatementSpan {
            start: statement.start,
            end: statement.end,
        })
        .collect();
    Analysis { statements }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gives_the_place_of_each_statement() {
        let request = AnalyzeRequest {
            sql: "SELECT 1;\n-- é\nSELECT 2".to_string(),
        };
        assert_eq!(
            analyze(&request).statements,
            vec![
                StatementSpan { start: 0, end: 9 },
                StatementSpan { start: 15, end: 23 },
            ]
        );
    }
}
