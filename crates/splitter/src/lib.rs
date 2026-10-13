//! Découpeur de requêtes : sépare un texte SQL en requêtes, et trouve celle
//! qui se trouve sous le curseur (Ctrl-Entrée).
//!
//! Le découpage suit les règles lexicales de MySQL et MariaDB, sans analyser
//! le SQL : un `;` sépare deux requêtes, sauf dans une chaîne (`'…'`, `"…"`),
//! un identifiant protégé (`` `…` ``) ou un commentaire (`-- …`, `# …`,
//! `/* … */`). La commande `DELIMITER` du client `mysql` est reconnue : elle
//! change le séparateur (par exemple `//` autour d'un `CREATE PROCEDURE`) et
//! n'est jamais envoyée au serveur.
//!
//! `split` donne les requêtes à exécuter ; `chunks` donne le texte entier en
//! morceaux (commandes `DELIMITER`, requêtes avec leurs commentaires et leur
//! séparateur), pour qui doit le réécrire sans rien perdre (l'enjoliveur).
//!
//! Les positions (`start`, `end`) sont comptées en unités UTF-16, comme celles
//! de l'éditeur de l'interface (CodeMirror) : un curseur se compare
//! directement à elles.
//!
//! Le parcours se fait octet par octet : tous les caractères qui comptent
//! (guillemets, séparateurs, marques de commentaire) sont en ASCII, et les
//! octets d'un caractère UTF-8 multi-octets valent toujours au moins 0x80,
//! sans confusion possible.

/// Séparateur de requêtes par défaut.
const DEFAULT_DELIMITER: &str = ";";

/// Requête découpée dans le texte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    /// Texte de la requête, sans son séparateur ni les commentaires et espaces qui la précèdent.
    pub sql: String,
    /// Début de la requête dans le texte, en unités UTF-16 : premier caractère significatif.
    pub start: usize,
    /// Fin de la requête dans le texte, en unités UTF-16, séparateur compris.
    pub end: usize,
}

/// Morceau du texte, dans l'ordre : mis bout à bout, les morceaux redonnent
/// le texte, aux blancs près autour des commandes `DELIMITER`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chunk {
    /// Commande `DELIMITER` du client `mysql`.
    Command {
        /// Ligne de la commande, sans les blancs qui l'entourent.
        line: String,
        /// Nouveau séparateur.
        delimiter: String,
    },
    /// Texte d'une requête, avec les espaces et commentaires qui la précèdent.
    Sql {
        /// Texte, sans son séparateur ; parfois fait seulement de blancs et
        /// de commentaires.
        text: String,
        /// Séparateur qui le termine (`;`, `//`...), vide en fin de texte.
        delimiter: String,
    },
}

/// Ce que le parcours du texte rencontre.
enum Event {
    /// Commande `DELIMITER`, de l'octet `start` à la fin de sa ligne `end`.
    Command {
        start: usize,
        end: usize,
        delimiter: String,
    },
    /// Requête (ou blancs et commentaires seuls) terminée par `delimiter`.
    Statement {
        /// Début du morceau : fin du séparateur ou de la commande précédente.
        chunk_start: usize,
        /// Premier octet significatif, absent s'il n'y a que blancs et commentaires.
        first: Option<usize>,
        /// Début du séparateur, ou fin du texte.
        body_end: usize,
        /// Fin du séparateur.
        end: usize,
        delimiter: String,
    },
}

/// Découpe `text` en requêtes. Les morceaux vides (espaces, commentaires seuls) sont ignorés.
pub fn split(text: &str) -> Vec<Statement> {
    scan(text)
        .into_iter()
        .filter_map(|event| match event {
            Event::Statement {
                first: Some(first),
                body_end,
                end,
                ..
            } => Some(statement(text, first, body_end, end)),
            _ => None,
        })
        .collect()
}

/// Découpe `text` en morceaux, sans rien perdre : commandes `DELIMITER`,
/// requêtes avec leurs commentaires, blancs et commentaires de fin de texte.
pub fn chunks(text: &str) -> Vec<Chunk> {
    scan(text)
        .into_iter()
        .map(|event| match event {
            Event::Command {
                start,
                end,
                delimiter,
            } => Chunk::Command {
                line: text[start..end].trim().to_string(),
                delimiter,
            },
            Event::Statement {
                chunk_start,
                body_end,
                delimiter,
                ..
            } => Chunk::Sql {
                text: text[chunk_start..body_end].to_string(),
                delimiter,
            },
        })
        .collect()
}

/// Parcourt `text` et relève, dans l'ordre, les commandes `DELIMITER` et
/// les morceaux qui se terminent par un séparateur ou par la fin du texte.
fn scan(text: &str) -> Vec<Event> {
    let bytes = text.as_bytes();
    let mut events = Vec::new();
    let mut delimiter = DEFAULT_DELIMITER.to_string();
    // Début du morceau en cours, et son premier octet significatif s'il y en a un.
    let mut chunk_start = 0;
    let mut start: Option<usize> = None;
    let mut i = 0;

    while i < bytes.len() {
        if start.is_none()
            && at_line_start(bytes, i)
            && let Some((new_delimiter, line_end)) = delimiter_command(text, i)
        {
            // Commentaires qui précèdent la commande : un morceau à part.
            if !text[chunk_start..i].trim().is_empty() {
                events.push(Event::Statement {
                    chunk_start,
                    first: None,
                    body_end: i,
                    end: i,
                    delimiter: String::new(),
                });
            }
            events.push(Event::Command {
                start: i,
                end: line_end,
                delimiter: new_delimiter.clone(),
            });
            delimiter = new_delimiter;
            i = line_end;
            chunk_start = i;
            continue;
        }
        if text[i..].starts_with(delimiter.as_str()) {
            events.push(Event::Statement {
                chunk_start,
                first: start.take(),
                body_end: i,
                end: i + delimiter.len(),
                delimiter: delimiter.clone(),
            });
            i += delimiter.len();
            chunk_start = i;
            continue;
        }
        match bytes[i] {
            b' ' | b'\t' | b'\r' | b'\n' => i += 1,
            b'-' if is_dash_comment(bytes, i) => i = line_end(bytes, i),
            b'#' => i = line_end(bytes, i),
            b'/' if bytes.get(i + 1) == Some(&b'*') => i = block_comment_end(bytes, i),
            quote @ (b'\'' | b'"' | b'`') => {
                start.get_or_insert(i);
                i = quoted_end(bytes, i, quote);
            }
            _ => {
                start.get_or_insert(i);
                i += 1;
            }
        }
    }
    if start.is_some() || !text[chunk_start..].trim().is_empty() {
        events.push(Event::Statement {
            chunk_start,
            first: start,
            body_end: bytes.len(),
            end: bytes.len(),
            delimiter: String::new(),
        });
    }
    events
}

/// Indice de la requête sous le curseur `cursor` (en unités UTF-16).
///
/// C'est la dernière requête qui commence avant le curseur : un curseur placé
/// entre deux requêtes (après le séparateur de l'une, avant le début de la
/// suivante) désigne la précédente. Avant la première requête, c'est la
/// première. `None` s'il n'y a aucune requête.
pub fn statement_at(statements: &[Statement], cursor: usize) -> Option<usize> {
    if statements.is_empty() {
        return None;
    }
    Some(
        statements
            .iter()
            .rposition(|statement| statement.start <= cursor)
            .unwrap_or(0),
    )
}

/// Construit une requête à partir de ses positions en octets.
fn statement(text: &str, first: usize, body_end: usize, end: usize) -> Statement {
    Statement {
        sql: text[first..body_end].trim_end().to_string(),
        start: utf16_offset(text, first),
        end: utf16_offset(text, end),
    }
}

/// Position en unités UTF-16 de l'octet `byte` du texte.
fn utf16_offset(text: &str, byte: usize) -> usize {
    text[..byte].encode_utf16().count()
}

/// Indique si l'octet `i` n'est précédé, sur sa ligne, que d'espaces.
fn at_line_start(bytes: &[u8], i: usize) -> bool {
    bytes[..i]
        .iter()
        .rev()
        .take_while(|&&byte| byte != b'\n')
        .all(|&byte| byte == b' ' || byte == b'\t' || byte == b'\r')
}

/// Reconnaît une commande `DELIMITER x` qui commence à l'octet `i` (espaces
/// compris). Renvoie le nouveau séparateur et la fin de la ligne.
fn delimiter_command(text: &str, i: usize) -> Option<(String, usize)> {
    let end = line_end(text.as_bytes(), i);
    let line = text[i..end].trim();
    let (keyword, rest) = line.split_once(char::is_whitespace)?;
    if !keyword.eq_ignore_ascii_case("delimiter") {
        return None;
    }
    let new_delimiter = rest.split_whitespace().next()?;
    Some((new_delimiter.to_string(), end))
}

/// Indique si `--` à l'octet `i` ouvre un commentaire : il doit être suivi
/// d'un espace, d'un caractère de contrôle ou de la fin du texte (règle MySQL).
fn is_dash_comment(bytes: &[u8], i: usize) -> bool {
    bytes.get(i + 1) == Some(&b'-') && bytes.get(i + 2).is_none_or(|&byte| byte <= b' ')
}

/// Position du retour à la ligne qui termine la ligne de l'octet `i`, ou fin du texte.
fn line_end(bytes: &[u8], i: usize) -> usize {
    bytes[i..]
        .iter()
        .position(|&byte| byte == b'\n')
        .map_or(bytes.len(), |offset| i + offset)
}

/// Position qui suit la fin du commentaire `/* … */` ouvert à l'octet `i`.
fn block_comment_end(bytes: &[u8], i: usize) -> usize {
    bytes[i + 2..]
        .windows(2)
        .position(|pair| pair == b"*/")
        .map_or(bytes.len(), |offset| i + 2 + offset + 2)
}

/// Position qui suit la fin de la chaîne ou de l'identifiant ouvert par `quote` à l'octet `i`.
///
/// Un guillemet doublé reste dans la chaîne. Dans `'…'` et `"…"`, la barre
/// oblique inverse protège le caractère suivant ; pas dans `` `…` ``.
fn quoted_end(bytes: &[u8], i: usize, quote: u8) -> usize {
    let mut j = i + 1;
    while j < bytes.len() {
        match bytes[j] {
            b'\\' if quote != b'`' => j += 2,
            byte if byte == quote => {
                if bytes.get(j + 1) == Some(&quote) {
                    j += 2;
                } else {
                    return j + 1;
                }
            }
            _ => j += 1,
        }
    }
    bytes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sqls(text: &str) -> Vec<String> {
        split(text)
            .into_iter()
            .map(|statement| statement.sql)
            .collect()
    }

    #[test]
    fn splits_on_semicolons() {
        assert_eq!(
            sqls("select * from xlist.video;\nselect * from xlist.person;\n"),
            ["select * from xlist.video", "select * from xlist.person"]
        );
        assert_eq!(sqls("SELECT 1"), ["SELECT 1"]);
        assert!(sqls("  ;; -- rien\n").is_empty());
    }

    #[test]
    fn ignores_semicolons_in_strings_identifiers_and_comments() {
        assert_eq!(
            sqls("SELECT 'a;b', \"c;d\", `e;f`; SELECT 'it''s; ok', 'x\\';y'"),
            [
                "SELECT 'a;b', \"c;d\", `e;f`",
                "SELECT 'it''s; ok', 'x\\';y'"
            ]
        );
        assert_eq!(
            sqls("SELECT 1 -- fin; pas ici\n; # rien; non plus\nSELECT /* ; */ 2"),
            ["SELECT 1 -- fin; pas ici", "SELECT /* ; */ 2"]
        );
    }

    #[test]
    fn double_dash_needs_a_space_to_open_a_comment() {
        assert_eq!(sqls("SELECT 5--2; SELECT 3"), ["SELECT 5--2", "SELECT 3"]);
    }

    #[test]
    fn statements_start_after_leading_comments() {
        let statements = split("-- titre\n/* note */ SELECT 1;");
        assert_eq!(statements[0].sql, "SELECT 1");
        assert_eq!(statements[0].start, 20);
        assert_eq!(statements[0].end, 29);
    }

    #[test]
    fn follows_the_delimiter_command() {
        let text = "DELIMITER //\nCREATE PROCEDURE p() BEGIN SELECT 1; SELECT 2; END //\ndelimiter ;\nCALL p();";
        assert_eq!(
            sqls(text),
            [
                "CREATE PROCEDURE p() BEGIN SELECT 1; SELECT 2; END",
                "CALL p()"
            ]
        );
    }

    #[test]
    fn cuts_the_whole_text_into_chunks() {
        let text = "-- purge\nDELIMITER //\nCREATE PROCEDURE p() BEGIN SELECT 1; END //\n delimiter ;\nCALL p(); # fin\n";
        assert_eq!(
            chunks(text),
            [
                Chunk::Sql {
                    text: "-- purge\n".to_string(),
                    delimiter: String::new(),
                },
                Chunk::Command {
                    line: "DELIMITER //".to_string(),
                    delimiter: "//".to_string(),
                },
                Chunk::Sql {
                    text: "\nCREATE PROCEDURE p() BEGIN SELECT 1; END ".to_string(),
                    delimiter: "//".to_string(),
                },
                // Les seuls blancs avant une commande ne font pas un morceau.
                Chunk::Command {
                    line: "delimiter ;".to_string(),
                    delimiter: ";".to_string(),
                },
                Chunk::Sql {
                    text: "\nCALL p()".to_string(),
                    delimiter: ";".to_string(),
                },
                Chunk::Sql {
                    text: " # fin\n".to_string(),
                    delimiter: String::new(),
                },
            ]
        );
    }

    #[test]
    fn counts_positions_in_utf16_units() {
        // « é » compte pour 1 unité UTF-16 (2 octets), « 😀 » pour 2 unités (4 octets).
        let statements = split("SELECT 'é😀';\nSELECT 2;");
        assert_eq!(statements[0].end, 13);
        assert_eq!(statements[1].start, 14);
    }

    #[test]
    fn finds_the_statement_under_the_cursor() {
        let text = "select * from xlist.video;\n\nselect * from xlist.person;\n";
        let statements = split(text);
        let second = text.find("select * from xlist.person").unwrap();

        assert_eq!(statement_at(&statements, 0), Some(0));
        assert_eq!(statement_at(&statements, 10), Some(0));
        // Ligne vide entre les deux : la précédente.
        assert_eq!(statement_at(&statements, 27), Some(0));
        assert_eq!(statement_at(&statements, second), Some(1));
        assert_eq!(statement_at(&statements, text.len()), Some(1));
        assert_eq!(statement_at(&[], 0), None);
    }

    #[test]
    fn cursor_before_the_first_statement_designates_it() {
        let statements = split("\n\n-- commentaire\nSELECT 1;");
        assert_eq!(statement_at(&statements, 0), Some(0));
    }
}
