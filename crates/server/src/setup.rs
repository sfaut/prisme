//! Réglages demandés au lancement : port d'écoute et adresse e-mail de
//! l'utilisateur (ticket #6).
//!
//! Au premier lancement (base interne absente du dossier de données) ou avec
//! `--setup`, Prisme pose les questions dans le terminal, en anglais ; Entrée
//! garde la valeur proposée, une réponse invalide est redemandée. Les
//! réponses sont enregistrées dans la base interne : le port dans
//! `server_settings`, l'adresse dans `users`. Les lancements suivants les
//! relisent sans rien demander.
//!
//! - Une option de la ligne de commande (`--listen`, `--user-email`) remplace
//!   sa question et n'est pas enregistrée.
//! - Sans terminal interactif (script, service), les valeurs par défaut
//!   s'appliquent sans question ; `--setup` y est refusé.
//! - Une base créée avant ces réglages garde les siens : port 8484 et
//!   utilisateur déjà enregistré.
//! - Le dossier de données n'est pas demandé : la base interne qui garde les
//!   réponses s'y trouve. Il se choisit par `--data-dir`.

use std::io::{BufRead, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener};

use anyhow::{Context, bail};
use rusqlite::{Connection, OptionalExtension, params};

use crate::config::Config;
use crate::store;

/// Adresse e-mail proposée au premier lancement.
pub const DEFAULT_EMAIL: &str = "me@local";

/// Longueurs permises d'une adresse e-mail, comme la contrainte de la table `users`.
const EMAIL_LENGTH: std::ops::RangeInclusive<usize> = 3..=254;

/// Réglages retenus pour ce lancement.
#[derive(Debug, PartialEq)]
pub struct Startup {
    /// Adresse d'écoute du serveur web.
    pub listen: SocketAddr,
    /// Identifiant de l'utilisateur unique.
    pub user_id: i64,
    /// Adresse e-mail de cet utilisateur.
    pub email: String,
}

/// Terminal où poser les questions.
pub struct Terminal<'a> {
    /// Réponses tapées par l'utilisateur, une par ligne.
    pub input: &'a mut dyn BufRead,
    /// Questions et messages d'erreur.
    pub output: &'a mut dyn Write,
    /// L'introduction a-t-elle été affichée (avant la première question) ?
    pub introduced: bool,
}

impl Terminal<'_> {
    /// Pose la question `question` (en anglais) jusqu'à obtenir une réponse
    /// que `parse` accepte ; `parse` explique sinon son refus, affiché avant
    /// de reposer la question. Une réponse vide garde `default`, affiché
    /// entre crochets. En fin d'entrée (Ctrl-Z, Ctrl-D), `default`
    /// s'applique ; s'il est lui-même refusé, Prisme s'arrête.
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
                Err(message) if ended => bail!("{question} : {message}"),
                Err(message) => writeln!(self.output, "  {message}")?,
            }
        }
    }
}

/// Établit le port d'écoute et l'utilisateur de ce lancement, en posant les
/// questions s'il y a lieu, puis enregistre les réponses dans la base interne
/// `db`.
///
/// `first_launch` indique que la base interne vient d'être créée ;
/// `terminal` est absent pour un lancement sans terminal interactif ;
/// `port_free` vérifie qu'un port proposé est libre (en production, la
/// fonction `port_free` de ce module).
pub fn settle(
    db: &Connection,
    config: &Config,
    first_launch: bool,
    mut terminal: Option<Terminal<'_>>,
    port_free: &dyn Fn(u16) -> Result<(), String>,
) -> anyhow::Result<Startup> {
    if config.setup && terminal.is_none() {
        bail!("--setup pose des questions : il demande un terminal interactif");
    }
    // Terminal où poser les questions, s'il y a des questions à poser.
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

/// Lit un numéro de port, de 1 à 65535.
pub fn parse_port(text: &str) -> Result<u16, String> {
    match text.parse::<u16>() {
        Ok(port) if port > 0 => Ok(port),
        _ => Err(format!(
            "Invalid port \"{text}\": enter a number from 1 to 65535."
        )),
    }
}

/// Vérifie qu'un port est libre sur 127.0.0.1, en y écoutant un instant :
/// un port déjà pris, ou réservé (sous 1024 sous Linux), est refusé.
pub fn port_free(port: u16) -> Result<(), String> {
    TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .map(drop)
        .map_err(|error| format!("Port {port} is not available: {error}."))
}

/// Vérifie une adresse e-mail, simplement : un `@` entre deux parties non
/// vides, aucun blanc, 254 caractères au plus. Le domaine peut ne pas avoir
/// de point (`me@local`).
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

/// Port enregistré dans la base interne.
fn stored_port(db: &Connection) -> rusqlite::Result<u16> {
    db.query_row(
        "SELECT listen_port FROM server_settings WHERE id = 1",
        [],
        |row| row.get(0),
    )
}

/// Enregistre le port d'écoute.
fn save_port(db: &Connection, port: u16) -> rusqlite::Result<()> {
    db.execute(
        "UPDATE server_settings SET listen_port = ?1 WHERE id = 1",
        params![port],
    )
    .map(drop)
}

/// Premier utilisateur enregistré (le seul en mode poste de développement).
fn first_user(db: &Connection) -> rusqlite::Result<Option<(i64, String)>> {
    db.query_row(
        "SELECT id, email FROM users ORDER BY id LIMIT 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
}

/// Change l'adresse e-mail d'un utilisateur (`--setup`).
fn rename_user(db: &Connection, id: i64, email: &str) -> anyhow::Result<()> {
    db.execute(
        "UPDATE users SET email = ?1 WHERE id = ?2",
        params![email, id],
    )
    .with_context(|| format!("adresse {email} : déjà prise par un autre utilisateur ?"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use clap::Parser;

    use super::*;

    /// Port proposé au premier lancement, aussi celui des bases créées avant
    /// ce réglage : valeur par défaut de la migration 0009.
    const DEFAULT_PORT: u16 = 8484;

    /// Port toujours libre, pour ne pas dépendre des ports du poste.
    fn always_free(_: u16) -> Result<(), String> {
        Ok(())
    }

    /// Lance `settle` avec des réponses tapées, et rend le résultat et le texte affiché.
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

        // Lancement suivant : rien n'est demandé, les réponses sont relues.
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
        // Base d'avant ce réglage : un utilisateur, le port par défaut.
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "sebastien.faut@gmail.com").unwrap();
        let (startup, output) = run(&db, &[], false, Some(""));
        let startup = startup.unwrap();
        assert_eq!(port_of(&startup), DEFAULT_PORT);
        assert_eq!(startup.user_id, user_id);
        assert_eq!(startup.email, "sebastien.faut@gmail.com");
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
