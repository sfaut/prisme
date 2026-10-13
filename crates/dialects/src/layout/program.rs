//! Mise en page des programmes stockés : corps des procédures, fonctions,
//! déclencheurs et événements, écrits entre deux commandes `DELIMITER`.
//!
//! Dans un programme, `;` termine une instruction du corps, pas la requête :
//! l'instruction suivante va à la ligne, au même retrait, sans ligne vide.
//! Les blocs passent en retrait :
//!
//! ```text
//! CREATE PROCEDURE purge_logs()
//! BEGIN
//!     DECLARE n INT;
//!     IF n > 0 THEN
//!         DELETE FROM logs;
//!     ELSEIF n < 0 THEN
//!         ...
//!     ELSE
//!         ...
//!     END IF;
//!     WHILE n > 0 DO
//!         ...
//!     END WHILE;
//!     CASE n
//!         WHEN 1 THEN
//!             ...
//!         ELSE
//!             ...
//!     END CASE;
//! END
//! ```
//!
//! `LOOP ... END LOOP` et `REPEAT ... UNTIL ... END REPEAT` suivent la même
//! règle ; une étiquette (`boucle: LOOP`) reste sur la ligne de son bloc.
//! Les mots qui structurent le programme (`DO`, `UNTIL`...) sont écrits en
//! capitales, même quand ils ne sont pas réservés, puisque leur rôle est
//! alors certain. Une expression `CASE ... END` dans une instruction n'est
//! pas un bloc : elle reste sur sa ligne.

use sqlparser::tokenizer::Token;

use super::{INDENT, Item, Printer, Query};

/// Bloc ouvert d'un programme.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum BlockKind {
    /// `BEGIN ... END`.
    Begin,
    /// `IF ... THEN ... [ELSEIF ... THEN ...] [ELSE ...] END IF`.
    If,
    /// `WHILE ... DO ... END WHILE`.
    While,
    /// `LOOP ... END LOOP`.
    Loop,
    /// `REPEAT ... UNTIL ... END REPEAT`.
    Repeat,
    /// `CASE [valeur] WHEN ... THEN ... [ELSE ...] END CASE`.
    Case,
}

/// Bloc ouvert, avec ses retraits.
pub(super) struct Block {
    kind: BlockKind,
    /// Retrait de la ligne qui ouvre le bloc, et de son `END`.
    indent: usize,
    /// Retrait des instructions du bloc.
    body: usize,
}

impl Printer {
    /// Retrait des instructions du bloc en cours (0 hors de tout bloc).
    pub(super) fn body_indent(&self) -> usize {
        self.blocks.last().map_or(0, |block| block.body)
    }

    /// Fin d'une instruction du corps : `;`, puis l'instruction suivante à la
    /// ligne, au même retrait.
    pub(super) fn end_program_statement(&mut self, item: &Item) {
        self.push_glued(item.text);
        self.start_program_statement(self.body_indent());
    }

    /// Commence une nouvelle instruction à la ligne, au retrait `indent`.
    fn start_program_statement(&mut self, indent: usize) {
        self.newline(indent);
        self.queries = vec![Query::new(0, indent)];
        self.statement_word.clear();
        self.statement_start = true;
    }

    /// Place un mot qui structure le programme. Renvoie `false` si le mot
    /// n'en est pas un ici : il est alors placé comme d'ordinaire.
    pub(super) fn place_program_word(&mut self, item: &Item, next: Option<&Item>) -> bool {
        let word = item.word();
        // Mot qui suit END : END IF, END WHILE...
        if std::mem::take(&mut self.end_pending)
            && matches!(word.as_str(), "IF" | "WHILE" | "LOOP" | "REPEAT" | "CASE")
        {
            self.push_spaced(&word);
            return true;
        }
        // Dans une expression CASE ... END, THEN, ELSE et END lui appartiennent.
        if !self.parens.is_empty()
            || self
                .queries
                .last()
                .is_some_and(|query| query.open_cases > 0)
        {
            return false;
        }
        let top = self.blocks.last().map(|block| block.kind);
        let at_start = std::mem::take(&mut self.statement_start);
        match word.as_str() {
            // Étiquette d'un bloc (`boucle: LOOP`) : le bloc suit sur la ligne.
            _ if at_start && next.is_some_and(|next| next.token == Token::Colon) => {
                self.push_spaced(item.text);
                self.statement_start = true;
                true
            }
            "BEGIN" => {
                if !self.label_before() {
                    self.newline(self.body_indent());
                }
                self.open_block(BlockKind::Begin, "BEGIN", true);
                true
            }
            "IF" | "WHILE" | "CASE" if at_start => {
                let kind = match word.as_str() {
                    "IF" => BlockKind::If,
                    "WHILE" => BlockKind::While,
                    _ => BlockKind::Case,
                };
                self.open_block(kind, &word, false);
                true
            }
            "LOOP" | "REPEAT" if at_start => {
                let kind = if word == "LOOP" {
                    BlockKind::Loop
                } else {
                    BlockKind::Repeat
                };
                self.open_block(kind, &word, true);
                true
            }
            "THEN" if matches!(top, Some(BlockKind::If | BlockKind::Case)) => {
                self.push_spaced("THEN");
                self.start_program_statement(self.body_indent());
                true
            }
            // En tête d'instruction, DO est l'instruction DO de MySQL (`DO SLEEP(1)`).
            "DO" if top == Some(BlockKind::While) && !at_start => {
                self.push_spaced("DO");
                self.start_program_statement(self.body_indent());
                true
            }
            "ELSEIF" if top == Some(BlockKind::If) => {
                let indent = self.blocks.last().map_or(0, |block| block.indent);
                self.newline(indent);
                self.push_spaced("ELSEIF");
                true
            }
            "WHEN" if top == Some(BlockKind::Case) => {
                let indent = self.blocks.last().map_or(0, |block| block.indent + INDENT);
                self.newline(indent);
                self.push_spaced("WHEN");
                true
            }
            "ELSE" if matches!(top, Some(BlockKind::If | BlockKind::Case)) => {
                let block = self.blocks.last().expect("bloc IF ou CASE");
                let indent = if block.kind == BlockKind::Case {
                    block.indent + INDENT
                } else {
                    block.indent
                };
                self.newline(indent);
                self.push_spaced("ELSE");
                self.start_program_statement(self.body_indent());
                true
            }
            "UNTIL" if top == Some(BlockKind::Repeat) => {
                let indent = self.blocks.last().map_or(0, |block| block.indent);
                self.newline(indent);
                self.push_spaced("UNTIL");
                self.queries = vec![Query::new(0, indent)];
                true
            }
            "END" if top.is_some() => {
                let block = self.blocks.pop().expect("bloc ouvert");
                self.newline(block.indent);
                self.push_spaced("END");
                self.queries = vec![Query::new(0, block.indent)];
                self.end_pending = true;
                true
            }
            _ => false,
        }
    }

    /// Ouvre un bloc sur la ligne en cours ; avec `body_now`, ses instructions
    /// commencent aussitôt à la ligne (`BEGIN`, `LOOP`, `REPEAT`), sinon après
    /// leur condition (`IF ... THEN`, `WHILE ... DO`, `CASE ... WHEN`).
    fn open_block(&mut self, kind: BlockKind, word: &str, body_now: bool) {
        let indent = self.current_line_indent();
        let body = match kind {
            BlockKind::Case => indent + 2 * INDENT,
            _ => indent + INDENT,
        };
        self.push_spaced(word);
        self.blocks.push(Block { kind, indent, body });
        if body_now {
            self.start_program_statement(body);
        } else {
            self.queries = vec![Query::new(0, indent)];
        }
    }

    /// La ligne en cours se termine-t-elle par une étiquette (`boucle:`) ?
    fn label_before(&self) -> bool {
        self.out.trim_end().ends_with(':')
    }
}
