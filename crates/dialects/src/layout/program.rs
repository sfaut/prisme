//! Layout of stored programs: bodies of procedures, functions, triggers and
//! events, written between two `DELIMITER` commands.
//!
//! In a program, `;` ends a statement of the body, not the query: the next
//! statement goes to a new line, at the same indentation, without an empty
//! line. Blocks are indented:
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
//! `LOOP ... END LOOP` and `REPEAT ... UNTIL ... END REPEAT` follow the same
//! rule; a label (`my_loop: LOOP`) stays on the line of its block. The words
//! that structure the program (`DO`, `UNTIL`...) are written in capitals, even
//! when they are not reserved, since their role is then certain. A
//! `CASE ... END` expression inside a statement is not a block: it stays on
//! its line.

use sqlparser::tokenizer::Token;

use super::{INDENT, Item, Printer, Query};

/// Open block of a program.
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
    /// `CASE [value] WHEN ... THEN ... [ELSE ...] END CASE`.
    Case,
}

/// Open block, with its indentations.
pub(super) struct Block {
    kind: BlockKind,
    /// Indentation of the line that opens the block, and of its `END`.
    indent: usize,
    /// Indentation of the statements of the block.
    body: usize,
}

impl Printer {
    /// Indentation of the statements of the current block (0 outside any block).
    pub(super) fn body_indent(&self) -> usize {
        self.blocks.last().map_or(0, |block| block.body)
    }

    /// End of a statement of the body: `;`, then the next statement on a new
    /// line, at the same indentation.
    pub(super) fn end_program_statement(&mut self, item: &Item) {
        self.push_glued(item.text);
        self.start_program_statement(self.body_indent());
    }

    /// Starts a new statement on a new line, at indentation `indent`.
    fn start_program_statement(&mut self, indent: usize) {
        self.newline(indent);
        self.queries.reset(Query::new(0, indent));
        self.statement_word.clear();
        self.statement_start = true;
    }

    /// Places a word that structures the program. Returns `false` if the word
    /// is not one here: it is then placed as usual.
    pub(super) fn place_program_word(&mut self, item: &Item, next: Option<&Item>) -> bool {
        let word = item.word();
        // Word that follows END: END IF, END WHILE...
        if std::mem::take(&mut self.end_pending)
            && matches!(word.as_str(), "IF" | "WHILE" | "LOOP" | "REPEAT" | "CASE")
        {
            self.push_spaced(&word);
            return true;
        }
        // In a CASE ... END expression, THEN, ELSE and END belong to it.
        if !self.parens.is_empty() || self.queries.current().open_cases > 0 {
            return false;
        }
        let top = self.blocks.last().map(|block| block.kind);
        let at_start = std::mem::take(&mut self.statement_start);
        match word.as_str() {
            // Label of a block (`my_loop: LOOP`): the block follows on the line.
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
            // At the start of a statement, DO is MySQL's DO statement (`DO SLEEP(1)`).
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
                let block = self
                    .blocks
                    .last()
                    .expect("ELSE is only matched under an open IF or CASE block");
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
                self.queries.reset(Query::new(0, indent));
                true
            }
            "END" if top.is_some() => {
                let block = self
                    .blocks
                    .pop()
                    .expect("END is only matched while a block is open");
                self.newline(block.indent);
                self.push_spaced("END");
                self.queries.reset(Query::new(0, block.indent));
                self.end_pending = true;
                true
            }
            _ => false,
        }
    }

    /// Opens a block on the current line; with `body_now`, its statements start
    /// right away on a new line (`BEGIN`, `LOOP`, `REPEAT`), otherwise after
    /// their condition (`IF ... THEN`, `WHILE ... DO`, `CASE ... WHEN`).
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
            self.queries.reset(Query::new(0, indent));
        }
    }

    /// Does the current line end with a label (`my_loop:`)?
    fn label_before(&self) -> bool {
        self.out.trim_end().ends_with(':')
    }
}
