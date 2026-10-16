//! Settings asked at launch: listening port and email address of the user
//! (ticket #6).
//!
//! On first launch (internal database missing from the data folder) or with
//! `--setup`, Prisme asks the questions in the terminal; Enter keeps the
//! proposed value, an invalid answer is asked again. The answers are saved in
//! the internal database: the port in `server_settings`, the address in
//! `users`. The next launches read them again without asking anything.
//!
//! - A command-line option (`--listen`, `--user-email`) replaces its question
//!   and is not saved.
//! - Without an interactive terminal (script, service), the defaults apply
//!   without any question; `--setup` is refused there.
//! - A database created before these settings keeps its own: port 8484 and
//!   the user already saved.
//! - The data folder is not asked: the internal database that keeps the
//!   answers lives in it. It is chosen with `--data-dir`.

use std::io::{BufRead, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener};

use anyhow::{Context, bail};
use rusqlite::{Connection, OptionalExtension, params};

use crate::config::Config;
use crate::store;

/// Email address proposed on first launch.
pub const DEFAULT_EMAIL: &str = "me@local";

/// Allowed lengths of an email address, like the constraint of the `users` table.
const EMAIL_LENGTH: std::ops::RangeInclusive<usize> = 3..=254;

/// Settings kept for this launch.
#[derive(Debug, PartialEq)]
pub struct Startup {
    /// Listening address of the web server.
    pub listen: SocketAddr,
    /// Id of the single user.
    pub user_id: i64,
    /// Email address of this user.
    pub email: String,
}

/// Terminal where the questions are asked.
pub struct Terminal<'a> {
    /// Answers typed by the user, one per line.
    pub input: &'a mut dyn BufRead,
    /// Questions and error messages.
    pub output: &'a mut dyn Write,
    /// Has the introduction been shown (before the first question)?
    pub introduced: bool,
}

impl Terminal<'_> {
    /// Asks `question` until `parse` accepts the answer; otherwise `parse`
    /// explains its refusal, shown before asking again. An empty answer keeps
    /// `default`, shown between brackets. At the end of input (Ctrl+Z,
    /// Ctrl+D), `default` applies; if it is refused itself, Prisme stops.
    fn ask<T>(
        &mut self,
        question: &str,
        default: &str,
        parse: impl Fn(&str) -> Result<T, String>,
    ) -> anyhow::Result<T> {
        if !self.introduced {
            writeln!(
                self.output,
                "Prisme setup. Press Enter to keep the value in brackets."
            )?;
            self.introduced = true;
        }
        loop {
            write!(self.output, "{question} [{default}]: ")?;
            self.output.flush()?;
            let mut line = String::new();
            let ended = self.input.read_line(&mut line)? == 0;
            let answer = match line.trim() {
                "" => default,
                typed => typed,
            };
            match parse(answer) {
                Ok(value) => return Ok(value),
                Err(message) if ended => bail!("{question}: {message}"),
                Err(message) => writeln!(self.output, "  {message}")?,
            }
        }
    }
}

/// Settles the listening port and the user of this launch, asking the
/// questions when relevant, then saves the answers in the internal database
/// `db`.
///
/// `first_launch` tells that the internal database has just been created;
/// `terminal` is absent for a launch without an interactive terminal;
/// `port_free` checks that a proposed port is free (in production, the
/// `port_free` function of this module).
pub fn settle(
    db: &Connection,
    config: &Config,
    first_launch: bool,
    mut terminal: Option<Terminal<'_>>,
    port_free: &dyn Fn(u16) -> Result<(), String>,
) -> anyhow::Result<Startup> {
    if config.setup && terminal.is_none() {
        bail!("--setup asks questions: it requires an interactive terminal");
    }
    // Terminal where the questions are asked, if there are questions to ask.
    let mut asked = if first_launch || config.setup {
        terminal.as_mut()
    } else {
        None
    };

    let listen = match config.listen {
        Some(listen) => listen,
        None => {
            let mut port = stored_port(db)?;
            if let Some(terminal) = asked.as_deref_mut() {
                port = terminal.ask("Listening port", &port.to_string(), |answer| {
                    let port = parse_port(answer)?;
                    port_free(port)?;
                    Ok(port)
                })?;
                save_port(db, port)?;
            }
            SocketAddr::from((Ipv4Addr::LOCALHOST, port))
        }
    };

    let (user_id, email) = match &config.user_email {
        Some(email) => (store::ensure_user(db, email)?, email.clone()),
        None => {
            let existing = first_user(db)?;
            match (asked, existing) {
                (Some(terminal), Some((id, current))) => {
                    let email = terminal.ask("Email address", &current, parse_email)?;
                    rename_user(db, id, &email)?;
                    (id, email)
                }
                (Some(terminal), None) => {
                    let email = terminal.ask("Email address", DEFAULT_EMAIL, parse_email)?;
                    (store::ensure_user(db, &email)?, email)
                }
                (None, Some(user)) => user,
                (None, None) => (
                    store::ensure_user(db, DEFAULT_EMAIL)?,
                    DEFAULT_EMAIL.to_string(),
                ),
            }
        }
    };

    Ok(Startup {
        listen,
        user_id,
        email,
    })
}

/// Reads a port number, from 1 to 65535.
pub fn parse_port(text: &str) -> Result<u16, String> {
    match text.parse::<u16>() {
        Ok(port) if port > 0 => Ok(port),
        _ => Err(format!(
            "Invalid port \"{text}\": enter a number from 1 to 65535."
        )),
    }
}

/// Checks that a port is free on 127.0.0.1, by listening on it for a moment:
/// a port already taken, or reserved (below 1024 on Linux), is refused.
pub fn port_free(port: u16) -> Result<(), String> {
    TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .map(drop)
        .map_err(|error| format!("Port {port} is not available: {error}."))
}

/// Checks an email address, simply: an `@` between two non-empty parts, no
/// whitespace, 254 characters at most. The domain may have no dot
/// (`me@local`).
pub fn parse_email(text: &str) -> Result<String, String> {
    let well_formed = text.split_once('@').is_some_and(|(name, domain)| {
        !name.is_empty() && !domain.is_empty() && !domain.contains('@')
    }) && !text.chars().any(char::is_whitespace)
        && EMAIL_LENGTH.contains(&text.chars().count());
    if well_formed {
        Ok(text.to_string())
    } else {
        Err(format!(
            "Invalid email address \"{text}\": expected name@domain, without spaces."
        ))
    }
}

/// Port saved in the internal database.
fn stored_port(db: &Connection) -> rusqlite::Result<u16> {
    db.query_row(
        "SELECT listen_port FROM server_settings WHERE id = 1",
        [],
        |row| row.get(0),
    )
}

/// Saves the listening port.
fn save_port(db: &Connection, port: u16) -> rusqlite::Result<()> {
    db.execute(
        "UPDATE server_settings SET listen_port = ?1 WHERE id = 1",
        params![port],
    )
    .map(drop)
}

/// First saved user (the only one in workstation mode).
fn first_user(db: &Connection) -> rusqlite::Result<Option<(i64, String)>> {
    db.query_row(
        "SELECT id, email FROM users ORDER BY id LIMIT 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
}

/// Changes the email address of a user (`--setup`).
fn rename_user(db: &Connection, id: i64, email: &str) -> anyhow::Result<()> {
    db.execute(
        "UPDATE users SET email = ?1 WHERE id = ?2",
        params![email, id],
    )
    .with_context(|| format!("address {email}: already taken by another user?"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use clap::Parser;

    use super::*;

    /// Port proposed on first launch, also that of the databases created
    /// before this setting: default value of migration 0009.
    const DEFAULT_PORT: u16 = 8484;

    /// Port always free, so as not to depend on the machine's ports.
    fn always_free(_: u16) -> Result<(), String> {
        Ok(())
    }

    /// Runs `settle` with typed answers, and returns the result and the text shown.
    fn run(
        db: &Connection,
        args: &[&str],
        first_launch: bool,
        answers: Option<&str>,
    ) -> (anyhow::Result<Startup>, String) {
        let config = Config::parse_from([&["prisme"][..], args].concat());
        let mut input = Cursor::new(answers.unwrap_or("").as_bytes().to_vec());
        let mut output = Vec::new();
        let terminal = answers.map(|_| Terminal {
            input: &mut input,
            output: &mut output,
            introduced: false,
        });
        let result = settle(db, &config, first_launch, terminal, &always_free);
        (result, String::from_utf8(output).unwrap())
    }

    fn port_of(startup: &Startup) -> u16 {
        startup.listen.port()
    }

    #[test]
    fn first_launch_asks_and_saves_the_answers() {
        let db = store::open_in_memory();
        let (startup, output) = run(&db, &[], true, Some("9000\nbob@example.com\n"));
        let startup = startup.unwrap();
        assert_eq!(startup.listen, "127.0.0.1:9000".parse().unwrap());
        assert_eq!(startup.email, "bob@example.com");
        assert!(output.contains("Listening port [8484]: "));
        assert!(output.contains("Email address [me@local]: "));

        // Next launch: nothing is asked, the answers are read again.
        let (next, output) = run(&db, &[], false, Some(""));
        let next = next.unwrap();
        assert_eq!(port_of(&next), 9000);
        assert_eq!(next.user_id, startup.user_id);
        assert_eq!(next.email, "bob@example.com");
        assert!(output.is_empty());
    }

    #[test]
    fn empty_answers_keep_the_defaults() {
        let db = store::open_in_memory();
        let startup = run(&db, &[], true, Some("\n\n")).0.unwrap();
        assert_eq!(port_of(&startup), DEFAULT_PORT);
        assert_eq!(startup.email, DEFAULT_EMAIL);
    }

    #[test]
    fn invalid_answers_are_asked_again() {
        let db = store::open_in_memory();
        let (startup, output) = run(
            &db,
            &[],
            true,
            Some("abc\n0\n70000\n9000\nno-at\na b@c\nme@local\n"),
        );
        assert_eq!(port_of(&startup.unwrap()), 9000);
        assert_eq!(output.matches("Invalid port").count(), 3);
        assert_eq!(output.matches("Invalid email address").count(), 2);
    }

    #[test]
    fn a_busy_port_is_refused() {
        let db = store::open_in_memory();
        let config = Config::parse_from(["prisme"]);
        let mut input = Cursor::new(b"8484\n8485\n\n".to_vec());
        let mut output = Vec::new();
        let terminal = Terminal {
            input: &mut input,
            output: &mut output,
            introduced: false,
        };
        let busy = |port: u16| {
            if port == 8484 {
                Err("busy".to_string())
            } else {
                Ok(())
            }
        };
        let startup = settle(&db, &config, true, Some(terminal), &busy).unwrap();
        assert_eq!(port_of(&startup), 8485);
        assert!(String::from_utf8(output).unwrap().contains("  busy"));
    }

    #[test]
    fn without_terminal_the_defaults_apply() {
        let db = store::open_in_memory();
        let startup = run(&db, &[], true, None).0.unwrap();
        assert_eq!(port_of(&startup), DEFAULT_PORT);
        assert_eq!(startup.email, DEFAULT_EMAIL);
    }

    #[test]
    fn setup_needs_a_terminal() {
        let db = store::open_in_memory();
        assert!(run(&db, &["--setup"], false, None).0.is_err());
    }

    #[test]
    fn an_existing_installation_keeps_its_settings() {
        // Database from before this setting: one user, the default port.
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "owner@example.com").unwrap();
        let (startup, output) = run(&db, &[], false, Some(""));
        let startup = startup.unwrap();
        assert_eq!(port_of(&startup), DEFAULT_PORT);
        assert_eq!(startup.user_id, user_id);
        assert_eq!(startup.email, "owner@example.com");
        assert!(output.is_empty());
    }

    #[test]
    fn setup_changes_the_settings_of_the_same_user() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "old@example.com").unwrap();
        let (startup, output) = run(&db, &["--setup"], false, Some("9100\nnew@example.com\n"));
        let startup = startup.unwrap();
        assert_eq!(port_of(&startup), 9100);
        assert_eq!(startup.user_id, user_id);
        assert_eq!(startup.email, "new@example.com");
        assert!(output.contains("Email address [old@example.com]: "));
    }

    #[test]
    fn command_line_options_replace_their_question_without_being_saved() {
        let db = store::open_in_memory();
        let args = [
            "--listen",
            "127.0.0.1:9200",
            "--user-email",
            "cli@example.com",
        ];
        let (startup, output) = run(&db, &args, true, Some(""));
        let startup = startup.unwrap();
        assert_eq!(port_of(&startup), 9200);
        assert_eq!(startup.email, "cli@example.com");
        assert!(output.is_empty());
        assert_eq!(stored_port(&db).unwrap(), DEFAULT_PORT);
    }

    #[test]
    fn checks_ports_and_email_addresses() {
        assert_eq!(parse_port("8484"), Ok(8484));
        assert!(parse_port("0").is_err());
        assert!(parse_port("65536").is_err());
        assert!(parse_port("port").is_err());

        assert!(parse_email("me@local").is_ok());
        assert!(parse_email("a.b@example.com").is_ok());
        assert!(parse_email("@local").is_err());
        assert!(parse_email("me@").is_err());
        assert!(parse_email("me").is_err());
        assert!(parse_email("a@b@c").is_err());
        assert!(parse_email("m e@local").is_err());
        assert!(parse_email(&format!("{}@local", "a".repeat(260))).is_err());
    }

    #[test]
    fn a_taken_port_is_seen_as_busy() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(port_free(port).is_err());
    }
}
