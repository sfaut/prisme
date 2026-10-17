//! SQL mode of the server, as far as it changes how a query is read
//! (`docs/QUESTIONS.md`, point 30).
//!
//! The mode is read on connection (`SELECT @@sql_mode`). The server gives
//! the individual modes, with the combination modes expanded; the
//! combination modes are expanded here as well, for a mode written by hand.

/// Flags of the SQL mode that change the reading of a query.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SqlMode {
    /// `ANSI_QUOTES`: `"..."` is an identifier, not a string.
    pub ansi_quotes: bool,
    /// `NO_BACKSLASH_ESCAPES`: the backslash is an ordinary character in
    /// strings.
    pub no_backslash_escapes: bool,
    /// `PIPES_AS_CONCAT`: `||` concatenates instead of `OR`.
    pub pipes_as_concat: bool,
    /// `IGNORE_SPACE`: spaces allowed between a function name and its
    /// parenthesis, the names of the built-in functions becoming reserved.
    pub ignore_space: bool,
    /// `HIGH_NOT_PRECEDENCE`: `NOT a BETWEEN b AND c` reads `(NOT a)
    /// BETWEEN b AND c`.
    pub high_not_precedence: bool,
    /// `REAL_AS_FLOAT`: `REAL` is `FLOAT` instead of `DOUBLE`.
    pub real_as_float: bool,
    /// `ORACLE` (MariaDB): the Oracle syntax, refused by Prisme.
    pub oracle: bool,
}

impl SqlMode {
    /// Reads the value of `@@sql_mode`: modes separated by commas, in any
    /// case. Modes without effect on the reading are ignored.
    pub fn parse(text: &str) -> Self {
        let mut mode = Self::default();
        for name in text.split(',').map(str::trim) {
            mode.add(&name.to_ascii_uppercase());
        }
        mode
    }

    /// Adds a mode, a combination mode adding its parts.
    fn add(&mut self, name: &str) {
        match name {
            "ANSI_QUOTES" => self.ansi_quotes = true,
            "NO_BACKSLASH_ESCAPES" => self.no_backslash_escapes = true,
            "PIPES_AS_CONCAT" => self.pipes_as_concat = true,
            "IGNORE_SPACE" => self.ignore_space = true,
            "HIGH_NOT_PRECEDENCE" => self.high_not_precedence = true,
            "REAL_AS_FLOAT" => self.real_as_float = true,
            "ANSI" => {
                for part in [
                    "REAL_AS_FLOAT",
                    "PIPES_AS_CONCAT",
                    "ANSI_QUOTES",
                    "IGNORE_SPACE",
                ] {
                    self.add(part);
                }
            }
            // Combination modes of MariaDB.
            "DB2" | "MAXDB" | "MSSQL" | "POSTGRESQL" => {
                for part in ["PIPES_AS_CONCAT", "ANSI_QUOTES", "IGNORE_SPACE"] {
                    self.add(part);
                }
            }
            "ORACLE" => {
                self.oracle = true;
                for part in ["PIPES_AS_CONCAT", "ANSI_QUOTES", "IGNORE_SPACE"] {
                    self.add(part);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_modes_given_by_the_server() {
        let mode = SqlMode::parse(
            "ONLY_FULL_GROUP_BY,STRICT_TRANS_TABLES,NO_ZERO_IN_DATE,ERROR_FOR_DIVISION_BY_ZERO",
        );
        assert_eq!(mode, SqlMode::default());
        let mode = SqlMode::parse("ansi_quotes, NO_BACKSLASH_ESCAPES");
        assert!(mode.ansi_quotes && mode.no_backslash_escapes && !mode.pipes_as_concat);
    }

    #[test]
    fn expands_the_combination_modes() {
        let ansi = SqlMode::parse("ANSI");
        assert!(
            ansi.ansi_quotes && ansi.pipes_as_concat && ansi.ignore_space && ansi.real_as_float
        );
        let oracle = SqlMode::parse("ORACLE");
        assert!(oracle.oracle && oracle.ansi_quotes);
    }
}
