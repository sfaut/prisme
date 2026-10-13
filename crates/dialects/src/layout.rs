//! Mise en page de l'enjoliveur : retours à la ligne, retraits et espaces.
//!
//! Règles (voir `docs/V1.md`, section 2.9) :
//!
//! - chaque clause principale commence une ligne : `SELECT`, `FROM`, les
//!   jointures, `WHERE`, `GROUP BY`, `HAVING`, `ORDER BY`, `LIMIT`, `UNION`... ;
//! - une liste de plusieurs colonnes après `SELECT` met une colonne par ligne,
//!   en retrait ; une seule colonne reste sur la ligne du `SELECT` ;
//! - plusieurs prédicats dans `WHERE`, `HAVING` ou `ON` : un par ligne, `AND`
//!   et `OR` en début de ligne, en retrait sous la clause (le `AND` d'un
//!   `BETWEEN` et ceux d'un `CASE` restent sur la ligne) ; `ON` reste sur la
//!   ligne de sa jointure ;
//! - quand il y en a plusieurs, les prédicats sont précédés de `TRUE` (si le
//!   premier lien est `AND`) ou de `FALSE` (si c'est `OR`), pour que chacun ait
//!   sa ligne, `AND` ou `OR` en tête (voir `predicate_prefix`) ;
//! - une sous-requête entre parenthèses est en retrait, sa parenthèse
//!   fermante sur sa propre ligne ;
//! - un espace autour des opérateurs binaires (`=`, `<`, `+`, `-`...), aucun
//!   après un opérateur unaire (`-1`) ; un espace après une virgule ;
//! - deux requêtes sont séparées par une ligne vide.
//!
//! Le texte de chaque jeton est repris tel quel (chaînes, commentaires,
//! identifiants entre accents graves, casse des mots) : seuls les espaces et
//! les retours à la ligne changent. Un nom de fonction reste collé à sa
//! parenthèse si c'était le cas, car MySQL l'exige pour ses fonctions.
//! Un texte qui contient la commande `DELIMITER`, ou que le découpeur en
//! jetons ne comprend pas, est renvoyé tel quel.

use sqlparser::tokenizer::{Token, Tokenizer, Whitespace};

use crate::Dialect;
use crate::keywords::is_structural_keyword;
use crate::limit::byte_offset;

/// Largeur d'un niveau de retrait.
const INDENT: usize = 4;

/// Clauses qui commencent une ligne.
const CLAUSES: &[&str] = &[
    "SELECT",
    "FROM",
    "WHERE",
    "GROUP",
    "HAVING",
    "ORDER",
    "LIMIT",
    "UNION",
    "INTERSECT",
    "EXCEPT",
    "WINDOW",
    "SET",
    "VALUES",
];

/// Mots qui commencent une jointure (et donc une ligne).
const JOIN_STARTS: &[&str] = &["JOIN", "INNER", "LEFT", "RIGHT", "CROSS", "NATURAL", "FULL"];

/// Mots qui précèdent `JOIN` dans une même jointure.
const JOIN_MODIFIERS: &[&str] = &[
    "INNER", "LEFT", "RIGHT", "OUTER", "CROSS", "NATURAL", "FULL",
];

/// Options d'un `SELECT`, qui restent sur sa ligne avant la liste des colonnes.
const SELECT_OPTIONS: &[&str] = &[
    "ALL",
    "DISTINCT",
    "DISTINCTROW",
    "HIGH_PRIORITY",
    "STRAIGHT_JOIN",
    "SQL_SMALL_RESULT",
    "SQL_BIG_RESULT",
    "SQL_BUFFER_RESULT",
    "SQL_NO_CACHE",
    "SQL_CACHE",
    "SQL_CALC_FOUND_ROWS",
];

/// Clauses qui terminent la liste des colonnes d'un `SELECT`.
const SELECT_LIST_ENDS: &[&str] = &[
    "FROM",
    "WHERE",
    "GROUP",
    "HAVING",
    "ORDER",
    "LIMIT",
    "UNION",
    "INTERSECT",
    "EXCEPT",
    "INTO",
    "WINDOW",
    "FOR",
];

/// Mots qui terminent une liste de prédicats (`WHERE`, `HAVING`, `ON`).
const PREDICATES_ENDS: &[&str] = &["ON", "USING", "FOR", "INTO", "LOCK"];

/// Texte ajouté juste après un mot-clef.
enum Injection {
    /// `TRUE` ou `FALSE` après `WHERE`, `HAVING` ou `ON`, puis le premier lien à la ligne.
    Prefix {
        constant: &'static str,
        connective: &'static str,
        indent: usize,
    },
}

/// Jeton à placer, avec son texte d'origine.
struct Item<'a> {
    token: Token,
    text: &'a str,
    /// Le texte d'origine avait-il un espace (ou un retour à la ligne) avant ce jeton ?
    space_before: bool,
}

impl Item<'_> {
    /// Mot non protégé, en capitales, ou chaîne vide pour un autre jeton.
    fn word(&self) -> String {
        match &self.token {
            Token::Word(word) if word.quote_style.is_none() => word.value.to_ascii_uppercase(),
            _ => String::new(),
        }
    }

    /// Le jeton est-il le mot `keyword` (non protégé) ?
    fn is(&self, keyword: &str) -> bool {
        self.word() == keyword
    }
}

/// Clause en cours d'une requête, pour placer les `AND` et `OR`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Clause {
    /// Liste des colonnes d'un `SELECT`.
    Select,
    /// `WHERE`, `HAVING` ou `ON` d'une jointure : prédicats en retrait sous la clause.
    Filter,
    /// Autre clause.
    Other,
}

/// Requête en cours de mise en page (la requête principale, ou une sous-requête).
struct Query {
    /// Profondeur de parenthèses où vivent les clauses de la requête.
    depth: usize,
    /// Retrait des clauses de la requête.
    indent: usize,
    /// Clause en cours.
    clause: Clause,
    /// La liste des colonnes du `SELECT` en cours a-t-elle plusieurs colonnes ?
    multi_columns: bool,
    /// La liste des colonnes attend-elle encore sa première colonne ?
    columns_pending: bool,
    /// Un `BETWEEN` attend son `AND`, qui reste sur la ligne.
    between_pending: bool,
    /// Nombre de `CASE` ouverts : leurs `AND` et `OR` restent sur la ligne.
    open_cases: usize,
    /// La dernière clause ouverte est-elle une jointure (qui attend son `ON`) ?
    joined: bool,
}

impl Query {
    fn new(depth: usize, indent: usize) -> Self {
        Self {
            depth,
            indent,
            clause: Clause::Other,
            multi_columns: false,
            columns_pending: false,
            between_pending: false,
            open_cases: 0,
            joined: false,
        }
    }
}

/// Met en page `sql`. Voir les règles en tête du module.
pub(crate) fn layout(sql: &str, dialect: Dialect) -> String {
    if has_delimiter_command(sql) {
        return sql.to_string();
    }
    let parser_dialect = dialect.parser_dialect();
    let Ok(tokens) = Tokenizer::new(parser_dialect.as_ref(), sql).tokenize_with_location() else {
        return sql.to_string();
    };

    let mut items = Vec::new();
    let mut space_before = false;
    for token in tokens {
        if matches!(
            token.token,
            Token::Whitespace(Whitespace::Space | Whitespace::Newline | Whitespace::Tab)
        ) {
            space_before = true;
            continue;
        }
        let (Some(start), Some(end)) = (
            byte_offset(sql, token.span.start),
            byte_offset(sql, token.span.end),
        ) else {
            return sql.to_string();
        };
        items.push(Item {
            token: token.token,
            text: &sql[start..end],
            space_before,
        });
        space_before = false;
    }
    Printer::default().print(&items)
}

/// Indique si le texte contient une commande `DELIMITER` du client `mysql`.
fn has_delimiter_command(sql: &str) -> bool {
    sql.lines().any(|line| {
        line.split_whitespace()
            .next()
            .is_some_and(|word| word.eq_ignore_ascii_case("delimiter"))
    })
}

/// Parenthèse ouverte.
struct Paren {
    /// Ouvre-t-elle une sous-requête ?
    subquery: bool,
    /// Retrait de la ligne où elle a été ouverte.
    line_indent: usize,
}

/// Écrit le texte mis en page, jeton par jeton.
#[derive(Default)]
struct Printer {
    out: String,
    queries: Vec<Query>,
    parens: Vec<Paren>,
    /// Le prochain jeton commence une ligne (après un commentaire de fin de ligne).
    newline_pending: bool,
    /// Le jeton précédent était un opérateur unaire, `(` ou `.` : le suivant s'y colle.
    glue_next: bool,
    /// Le jeton précédent était un opérateur binaire ou une virgule : le suivant
    /// est précédé d'un espace.
    space_next: bool,
    /// Texte à ajouter après le mot en cours de placement.
    injection: Option<Injection>,
}

impl Printer {
    /// Met en page tous les jetons et renvoie le texte.
    fn print(mut self, items: &[Item]) -> String {
        self.queries.push(Query::new(0, 0));
        for index in 0..items.len() {
            self.place(items, index);
        }
        self.out.trim_end().to_string()
    }

    /// Requête en cours (la plus intérieure).
    fn query(&mut self) -> &mut Query {
        self.queries
            .last_mut()
            .expect("au moins la requête principale")
    }

    /// Le jeton est-il au niveau des clauses de la requête en cours ?
    fn at_query_level(&self) -> bool {
        self.queries.last().map(|query| query.depth) == Some(self.parens.len())
    }

    /// Place le jeton `index`.
    fn place(&mut self, items: &[Item], index: usize) {
        let item = &items[index];
        let previous = index.checked_sub(1).map(|previous| &items[previous]);
        let next = items.get(index + 1);
        let word = item.word();

        if self.newline_pending {
            self.newline_pending = false;
            let indent = self.current_line_indent();
            self.newline(indent);
        }

        // Fin de requête : ligne vide avant la suivante.
        if matches!(item.token, Token::SemiColon) && self.parens.is_empty() {
            self.push_glued(item.text);
            self.out.push_str("\n\n");
            self.queries = vec![Query::new(0, 0)];
            return;
        }

        if let Token::Whitespace(comment) = &item.token {
            self.place_comment(item, comment);
            return;
        }

        if matches!(item.token, Token::LParen) {
            self.open_paren(item, previous, next);
            return;
        }
        if matches!(item.token, Token::RParen) {
            self.close_paren(item);
            return;
        }

        if self.at_query_level() && !word.is_empty() {
            let query = self.queries.last().expect("requête");
            let starts_predicates = word == "WHERE"
                || word == "HAVING"
                || (word == "ON" && query.joined && query.clause == Clause::Other);
            if starts_predicates
                && let Some((constant, connective)) = predicate_prefix(&items[index + 1..])
            {
                self.injection = Some(Injection::Prefix {
                    constant,
                    connective,
                    indent: query.indent + INDENT,
                });
            }
            let several_columns = word == "SELECT" && has_several_columns(&items[index + 1..]);
            self.place_keyword_line(&word, previous, next, several_columns);
        }

        // Première colonne d'une liste de plusieurs colonnes : à la ligne, en retrait.
        let query = self.query();
        if query.clause == Clause::Select
            && query.columns_pending
            && !SELECT_OPTIONS.contains(&word.as_str())
            && !item.is("SELECT")
        {
            query.columns_pending = false;
            if query.multi_columns {
                let indent = query.indent + INDENT;
                self.newline(indent);
            }
        }

        if matches!(item.token, Token::Comma) {
            self.push_glued(item.text);
            let query = self.queries.last().expect("requête");
            if query.clause == Clause::Select
                && query.multi_columns
                && query.depth == self.parens.len()
            {
                let indent = query.indent + INDENT;
                self.newline(indent);
            } else {
                self.space_next = true;
            }
            return;
        }

        if matches!(item.token, Token::Period) {
            self.push_glued(item.text);
            self.glue_next = true;
            return;
        }

        if is_operator(&item.token) {
            if previous.is_some_and(is_operand) {
                self.push_spaced(item.text);
                self.space_next = true;
            } else {
                // Opérateur unaire (-1, *) : collé à ce qui suit.
                self.push_spaced(item.text);
                self.glue_next = true;
            }
            return;
        }

        if item.space_before || !word.is_empty() {
            self.push_spaced(item.text);
        } else {
            self.push_glued(item.text);
        }

        match self.injection.take() {
            Some(Injection::Prefix {
                constant,
                connective,
                indent,
            }) => {
                self.push_spaced(constant);
                self.newline(indent);
                self.push_spaced(connective);
            }
            None => {}
        }
    }

    /// Passe à la ligne avant une clause, une jointure ou un `AND` / `OR`, et
    /// suit la clause en cours.
    fn place_keyword_line(
        &mut self,
        word: &str,
        previous: Option<&Item>,
        next: Option<&Item>,
        several_columns: bool,
    ) {
        let function_call =
            next.is_some_and(|next| matches!(next.token, Token::LParen) && !next.space_before);
        let query_indent = self.query().indent;

        if word == "CASE" {
            self.query().open_cases += 1;
            return;
        }
        if word == "END" && self.query().open_cases > 0 {
            self.query().open_cases -= 1;
            return;
        }
        if word == "BETWEEN" {
            self.query().between_pending = true;
            return;
        }

        if (word == "AND" || word == "OR") && self.query().open_cases == 0 {
            if word == "AND" && self.query().between_pending {
                self.query().between_pending = false;
                return;
            }
            if self.query().clause == Clause::Filter {
                self.newline(query_indent + INDENT);
            }
            return;
        }

        if word == "ON" && matches!(self.query().clause, Clause::Other) && self.after_join() {
            self.query().clause = Clause::Filter;
            return;
        }

        let joins = JOIN_STARTS.contains(&word)
            && !function_call
            && !(word == "JOIN"
                && previous
                    .is_some_and(|previous| JOIN_MODIFIERS.contains(&previous.word().as_str())))
            && !(word == "STRAIGHT_JOIN");
        if joins {
            self.newline(query_indent);
            let query = self.query();
            query.clause = Clause::Other;
            query.joined = true;
            return;
        }

        if CLAUSES.contains(&word) && !function_call {
            // BY suit GROUP et ORDER sur la même ligne.
            self.newline(query_indent);
            let query = self.query();
            query.between_pending = false;
            query.joined = false;
            query.clause = match word {
                "SELECT" => Clause::Select,
                "WHERE" | "HAVING" => Clause::Filter,
                _ => Clause::Other,
            };
            if word == "SELECT" {
                query.columns_pending = true;
                query.multi_columns = several_columns;
            }
        }
    }

    /// La dernière clause ouverte est-elle une jointure (pour reconnaître son `ON`) ?
    fn after_join(&self) -> bool {
        self.queries.last().is_some_and(|query| query.joined)
    }

    /// Place une parenthèse ouvrante ; une sous-requête passe en retrait.
    fn open_paren(&mut self, item: &Item, previous: Option<&Item>, next: Option<&Item>) {
        // Après un mot, l'espace d'origine est conservé : un nom de fonction
        // reste collé à sa parenthèse (MySQL l'exige pour ses fonctions).
        let after_word = previous.is_some_and(|previous| matches!(previous.token, Token::Word(_)));
        if !after_word || item.space_before {
            self.push_spaced(item.text);
        } else {
            self.push_glued(item.text);
        }
        self.glue_next = true;

        let subquery = next.is_some_and(|next| next.is("SELECT") || next.is("WITH"));
        let line_indent = self.current_line_indent();
        self.parens.push(Paren {
            subquery,
            line_indent,
        });
        if subquery {
            let indent = line_indent + INDENT;
            self.queries.push(Query::new(self.parens.len(), indent));
            self.newline(indent);
        }
    }

    /// Place une parenthèse fermante ; celle d'une sous-requête a sa propre ligne.
    fn close_paren(&mut self, item: &Item) {
        let Some(paren) = self.parens.pop() else {
            self.push_glued(item.text);
            return;
        };
        if paren.subquery {
            self.queries.pop();
            self.newline(paren.line_indent);
            self.out.push_str(item.text);
        } else {
            self.push_glued(item.text);
        }
    }

    /// Place un commentaire : un commentaire de fin de ligne force le retour à la ligne.
    fn place_comment(&mut self, item: &Item, comment: &Whitespace) {
        self.push_spaced(item.text.trim_end());
        if matches!(comment, Whitespace::SingleLineComment { .. }) {
            self.newline_pending = true;
        }
    }

    /// Ajoute un texte, précédé d'un espace si la ligne en cours n'est pas vide.
    fn push_spaced(&mut self, text: &str) {
        self.space_next = false;
        let glued = std::mem::take(&mut self.glue_next);
        let at_line_start =
            self.out.is_empty() || self.out.ends_with('\n') || self.out.ends_with(' ');
        if !glued && !at_line_start {
            self.out.push(' ');
        }
        self.out.push_str(text);
    }

    /// Ajoute un texte collé à ce qui précède.
    fn push_glued(&mut self, text: &str) {
        if std::mem::take(&mut self.space_next) {
            self.push_spaced(text);
            return;
        }
        self.glue_next = false;
        let trimmed = self.out.trim_end_matches(' ').len();
        if !self.out.ends_with('\n') && !self.out[..trimmed].ends_with('\n') {
            self.out.truncate(trimmed);
        }
        self.out.push_str(text);
    }

    /// Passe à la ligne avec le retrait `indent`, sauf en tout début de texte.
    fn newline(&mut self, indent: usize) {
        self.glue_next = false;
        self.space_next = false;
        let trimmed = self.out.trim_end_matches(' ').len();
        self.out.truncate(trimmed);
        if self.out.is_empty() {
            return;
        }
        if !self.out.ends_with('\n') {
            self.out.push('\n');
        }
        self.out.push_str(&" ".repeat(indent));
    }

    /// Retrait de la ligne en cours.
    fn current_line_indent(&self) -> usize {
        let line = self.out.rsplit('\n').next().unwrap_or("");
        line.len() - line.trim_start_matches(' ').len()
    }
}

/// Indique si la liste des colonnes d'un `SELECT` (les jetons qui le suivent)
/// compte plusieurs colonnes, séparées par une virgule à son niveau.
fn has_several_columns(items: &[Item]) -> bool {
    let mut depth = 0usize;
    for item in items {
        match item.token {
            Token::LParen => depth += 1,
            Token::RParen if depth == 0 => return false,
            Token::RParen => depth -= 1,
            Token::SemiColon if depth == 0 => return false,
            Token::Comma if depth == 0 => return true,
            _ if depth == 0 && SELECT_LIST_ENDS.contains(&item.word().as_str()) => return false,
            _ => {}
        }
    }
    false
}

/// Préfixe à ajouter devant une liste de prédicats (les jetons qui suivent
/// `WHERE`, `HAVING` ou `ON`), pour que chaque prédicat ait sa ligne :
/// `TRUE` puis `AND` si le premier lien de la liste est `AND`, `FALSE` puis
/// `OR` si c'est `OR`. `None` pour un seul prédicat, ou si le préfixe est déjà là.
///
/// Le sens de la requête ne change pas : en logique à trois valeurs,
/// `TRUE AND x` et `FALSE OR x` valent `x`, y compris quand `x` est NULL. Et
/// comme `AND` lie plus fort que `OR` (puis `XOR`), `TRUE AND p1 OR p2` se lit
/// `(TRUE AND p1) OR p2`, et `FALSE OR p1 AND p2` se lit `FALSE OR (p1 AND p2)` :
/// le préfixe ne s'attache qu'au premier terme, quel que soit le mélange.
fn predicate_prefix(items: &[Item]) -> Option<(&'static str, &'static str)> {
    let mut significant = items
        .iter()
        .filter(|item| !matches!(item.token, Token::Whitespace(_)));
    let first = significant.next()?.word();
    let second = significant.next().map(Item::word).unwrap_or_default();
    if (first == "TRUE" && second == "AND") || (first == "FALSE" && second == "OR") {
        return None;
    }

    let mut depth = 0usize;
    let mut open_cases = 0usize;
    let mut between_pending = false;
    for (index, item) in items.iter().enumerate() {
        match item.token {
            Token::LParen => depth += 1,
            Token::RParen if depth == 0 => return None,
            Token::RParen => depth -= 1,
            Token::SemiColon if depth == 0 => return None,
            _ => {}
        }
        if depth > 0 {
            continue;
        }
        let word = item.word();
        let function_call = items
            .get(index + 1)
            .is_some_and(|next| matches!(next.token, Token::LParen) && !next.space_before);
        let ends = CLAUSES.contains(&word.as_str())
            || SELECT_LIST_ENDS.contains(&word.as_str())
            || PREDICATES_ENDS.contains(&word.as_str())
            || (JOIN_STARTS.contains(&word.as_str()) && !function_call);
        if ends {
            return None;
        }
        match word.as_str() {
            "CASE" => open_cases += 1,
            "END" if open_cases > 0 => open_cases -= 1,
            "BETWEEN" => between_pending = true,
            "AND" if between_pending => between_pending = false,
            "AND" if open_cases == 0 => return Some(("TRUE", "AND")),
            "OR" if open_cases == 0 => return Some(("FALSE", "OR")),
            _ => {}
        }
    }
    None
}

/// Indique si un jeton est un opérateur, binaire ou unaire.
fn is_operator(token: &Token) -> bool {
    matches!(
        token,
        Token::Eq
            | Token::DoubleEq
            | Token::Neq
            | Token::Lt
            | Token::Gt
            | Token::LtEq
            | Token::GtEq
            | Token::Spaceship
            | Token::Plus
            | Token::Minus
            | Token::Mul
            | Token::Div
            | Token::Mod
            | Token::StringConcat
            | Token::Assignment
            | Token::Ampersand
            | Token::Pipe
            | Token::Caret
            | Token::ShiftLeft
            | Token::ShiftRight
            | Token::Arrow
            | Token::LongArrow
    )
}

/// Indique si un jeton termine un opérande : l'opérateur qui le suit est binaire.
fn is_operand(item: &Item) -> bool {
    match &item.token {
        Token::Word(word) => word.quote_style.is_some() || !is_structural_keyword(&word.value),
        Token::RParen | Token::Placeholder(_) | Token::Number(..) => true,
        Token::SingleQuotedString(_)
        | Token::DoubleQuotedString(_)
        | Token::NationalStringLiteral(_)
        | Token::HexStringLiteral(_)
        | Token::SingleQuotedByteStringLiteral(_)
        | Token::DoubleQuotedByteStringLiteral(_) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn laid_out(sql: &str) -> String {
        layout(sql, Dialect::MariaDb)
    }

    #[test]
    fn puts_each_clause_on_its_own_line() {
        assert_eq!(
            laid_out(
                "SELECT ALL * FROM video WHERE created_at >= CURRENT_DATE - INTERVAL 120 SECOND ORDER BY id DESC LIMIT 10;"
            ),
            "SELECT ALL *\nFROM video\nWHERE created_at >= CURRENT_DATE - INTERVAL 120 SECOND\nORDER BY id DESC\nLIMIT 10;"
        );
    }

    #[test]
    fn puts_one_column_per_line_when_there_are_several() {
        assert_eq!(
            laid_out("SELECT DISTINCT id, count(*), LEFT(name, 3) AS prefix FROM t"),
            "SELECT DISTINCT\n    id,\n    count(*),\n    LEFT(name, 3) AS prefix\nFROM t"
        );
    }

    #[test]
    fn puts_one_predicate_per_line_in_where_and_on() {
        let sql = "SELECT v.id FROM video v JOIN person p ON p.id = v.person_id AND p.active=1 \
                   LEFT JOIN tag t ON t.video_id = v.id WHERE v.year BETWEEN 2000 AND 2010 AND v.score>3 OR v.free";
        assert_eq!(
            laid_out(sql),
            "SELECT v.id\nFROM video v\nJOIN person p ON TRUE\n    AND p.id = v.person_id\n    AND p.active = 1\nLEFT JOIN tag t ON t.video_id = v.id\nWHERE TRUE\n    AND v.year BETWEEN 2000 AND 2010\n    AND v.score > 3\n    OR v.free"
        );
    }

    #[test]
    fn keeps_case_conditions_on_their_line() {
        assert_eq!(
            laid_out("SELECT CASE WHEN a AND b THEN 1 ELSE 0 END FROM t WHERE x AND y"),
            "SELECT CASE WHEN a AND b THEN 1 ELSE 0 END\nFROM t\nWHERE TRUE\n    AND x\n    AND y"
        );
    }

    #[test]
    fn prefixes_predicates_with_true_or_false() {
        assert_eq!(
            laid_out("SELECT * FROM t1 INNER JOIN t2 ON p1 AND p2 WHERE p3 OR p4"),
            "SELECT *\nFROM t1\nINNER JOIN t2 ON TRUE\n    AND p1\n    AND p2\nWHERE FALSE\n    OR p3\n    OR p4"
        );
        // Un seul prédicat : rien à ajouter.
        assert_eq!(
            laid_out("SELECT * FROM t WHERE a = 1"),
            "SELECT *\nFROM t\nWHERE a = 1"
        );
        // Préfixe déjà présent : il n'est pas doublé.
        assert_eq!(
            laid_out("SELECT * FROM t WHERE FALSE OR a OR b"),
            "SELECT *\nFROM t\nWHERE FALSE\n    OR a\n    OR b"
        );
        // Les liens d'une sous-requête ou d'une fonction ne comptent pas.
        assert_eq!(
            laid_out("SELECT * FROM t WHERE a IN (SELECT x FROM u WHERE y OR z)"),
            "SELECT *\nFROM t\nWHERE a IN (\n    SELECT x\n    FROM u\n    WHERE FALSE\n        OR y\n        OR z\n)"
        );
    }

    #[test]
    fn indents_subqueries() {
        assert_eq!(
            laid_out("SELECT id FROM video WHERE id IN (SELECT video_id FROM fav WHERE ok = 1)"),
            "SELECT id\nFROM video\nWHERE id IN (\n    SELECT video_id\n    FROM fav\n    WHERE ok = 1\n)"
        );
    }

    #[test]
    fn spaces_binary_operators_only() {
        assert_eq!(
            laid_out("SELECT a-b, -1, a - -1, t.*, x=-1 FROM t WHERE (a+b)*2>=c"),
            "SELECT\n    a - b,\n    -1,\n    a - -1,\n    t.*,\n    x = -1\nFROM t\nWHERE (a + b) * 2 >= c"
        );
    }

    #[test]
    fn keeps_comments_strings_and_function_parentheses() {
        assert_eq!(
            laid_out(
                "SELECT `a  b`, 'x   y' -- note\nFROM t /* c */ WHERE f (1) = concat('a',  'b')"
            ),
            "SELECT\n    `a  b`,\n    'x   y' -- note\nFROM t /* c */\nWHERE f (1) = concat('a', 'b')"
        );
    }

    #[test]
    fn separates_statements_with_a_blank_line() {
        assert_eq!(laid_out("SELECT 1; select 2;"), "SELECT 1;\n\nselect 2;");
    }

    #[test]
    fn leaves_delimiter_scripts_unchanged() {
        let sql = "DELIMITER //\nCREATE PROCEDURE p() BEGIN SELECT 1; END //";
        assert_eq!(laid_out(sql), sql);
    }
}
