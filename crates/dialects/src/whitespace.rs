//! Normalisation des blancs, première règle de l'enjoliveur.
//!
//! Un texte copié depuis un traitement de texte, une page web ou une
//! messagerie contient souvent des blancs « exotiques » : espace insécable,
//! espace fine, espace idéographique... Invisibles à l'écran, ils gênent le
//! découpage en jetons et la mise en page. Ils deviennent ici des espaces
//! normales, les séparateurs de ligne Unicode des retours à la ligne, et une
//! tabulation devient quatre espaces.
//!
//! Les chaînes (`'...'`, `"..."`) et les identifiants entre accents graves
//! restent intacts : un blanc y fait partie de la valeur, le changer
//! changerait la requête. Les commentaires sont normalisés comme le reste :
//! ils n'ont pas d'effet sur la requête.

/// Une tabulation devient ce nombre d'espaces.
const TAB_WIDTH: usize = 4;

/// Endroit du texte où se trouve le caractère lu.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Context {
    /// Code SQL ordinaire.
    Code,
    /// Chaîne ou identifiant délimité par ce caractère (`'`, `"` ou `` ` ``).
    Quoted(char),
    /// Commentaire jusqu'à la fin de la ligne (`-- ` ou `#`).
    LineComment,
    /// Commentaire entre `/*` et `*/`.
    BlockComment,
}

/// Remplace les blancs exotiques et les tabulations hors des chaînes et des
/// identifiants entre accents graves. Voir l'en-tête du module.
pub(crate) fn normalize_blanks(sql: &str) -> String {
    let mut result = String::with_capacity(sql.len());
    let mut context = Context::Code;
    let mut chars = sql.chars().peekable();
    while let Some(c) = chars.next() {
        let next = chars.peek().copied();
        match context {
            Context::Quoted(quote) => {
                result.push(c);
                if c == '\\' && quote != '`' {
                    // Caractère échappé (`\'`) : recopié tel quel, il ne ferme rien.
                    if let Some(escaped) = chars.next() {
                        result.push(escaped);
                    }
                } else if c == quote {
                    if next == Some(quote) {
                        // Délimiteur doublé (`''`) : un délimiteur dans la valeur.
                        result.push(chars.next().unwrap_or(quote));
                    } else {
                        context = Context::Code;
                    }
                }
                continue;
            }
            Context::Code => match c {
                '\'' | '"' | '`' => context = Context::Quoted(c),
                '#' => context = Context::LineComment,
                // MySQL n'ouvre un commentaire `--` que suivi d'un blanc.
                '-' if next == Some('-') => {
                    let mut ahead = chars.clone();
                    ahead.next();
                    if ahead.peek().is_none_or(|after| after.is_whitespace()) {
                        context = Context::LineComment;
                    }
                }
                '/' if next == Some('*') => context = Context::BlockComment,
                _ => {}
            },
            Context::LineComment => {
                if c == '\n' {
                    context = Context::Code;
                }
            }
            Context::BlockComment => {
                if c == '*' && next == Some('/') {
                    result.push(c);
                    result.push(chars.next().unwrap_or('/'));
                    context = Context::Code;
                    continue;
                }
            }
        }
        push_normalized(&mut result, c);
    }
    result
}

/// Ajoute `c` à `result`, normalisé s'il s'agit d'un blanc.
fn push_normalized(result: &mut String, c: char) {
    match c {
        '\t' => result.extend(std::iter::repeat_n(' ', TAB_WIDTH)),
        // Séparateurs de ligne et de paragraphe Unicode, « nouvelle ligne » (NEL).
        '\u{2028}' | '\u{2029}' | '\u{0085}' => result.push('\n'),
        // Tout autre blanc hors ASCII : espaces insécables, fines, cadratin...
        c if c.is_whitespace() && !c.is_ascii() => result.push(' '),
        c => result.push(c),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_exotic_blanks_and_tabs() {
        assert_eq!(
            normalize_blanks("SELECT\u{00A0}a,\u{202F}b\tFROM\u{3000}t\u{2028}WHERE x"),
            "SELECT a, b    FROM t\nWHERE x"
        );
    }

    #[test]
    fn keeps_strings_and_quoted_identifiers_intact() {
        let sql = "SELECT 'a\u{00A0}b', \"c\td\", `e\u{2009}f` FROM t";
        assert_eq!(normalize_blanks(sql), sql);
    }

    #[test]
    fn follows_escapes_and_doubled_quotes() {
        // La chaîne continue après \' et '' : le blanc qui suit y est encore.
        let sql = "SELECT 'it\\'s\u{00A0}', 'l''a\u{00A0}' ,\u{00A0}x";
        assert_eq!(
            normalize_blanks(sql),
            "SELECT 'it\\'s\u{00A0}', 'l''a\u{00A0}' , x"
        );
    }

    #[test]
    fn normalizes_comments_without_misreading_quotes() {
        // L'apostrophe d'un commentaire n'ouvre pas de chaîne.
        assert_eq!(
            normalize_blanks("-- it's\u{00A0}ok\nSELECT\u{00A0}1 /* l'a\tb */"),
            "-- it's ok\nSELECT 1 /* l'a    b */"
        );
        assert_eq!(normalize_blanks("# x'\nSELECT\u{00A0}1"), "# x'\nSELECT 1");
        // Sans blanc après --, c'est une double soustraction.
        assert_eq!(
            normalize_blanks("SELECT 1--1,\u{00A0}'a'"),
            "SELECT 1--1, 'a'"
        );
    }
}
