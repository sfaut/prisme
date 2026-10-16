//! Configuration of Prisme, read from the command line.
//!
//! The v1 only knows the workstation mode: a single user, without
//! authentication, hence listening limited to the local machine so that no
//! other machine of the network can reach the saved connections.

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, bail};
use clap::Parser;

/// Name of the data folder created in the user's data folder.
const DATA_DIR_NAME: &str = "Prisme";

/// How the first launch works, shown at the end of `prisme --help`.
const AFTER_HELP: &str = "\
First launch:
  When the internal database does not exist yet in the data folder, Prisme
  asks in the terminal for the listening port (8484 by default, on
  127.0.0.1) and the user's email address (me@local by default). The
  answers are saved in the internal database and read again on the next
  launches; --setup asks the questions again.
  A --listen or --user-email option replaces its question, without being
  saved. Without an interactive terminal, the defaults apply.";

/// Launch options of Prisme.
#[derive(Debug, Parser)]
#[command(
    name = "prisme",
    version,
    about = "Multi-database web client shaped like a notebook",
    after_help = AFTER_HELP,
    disable_version_flag = true
)]
pub struct Config {
    /// Print version
    // Prisme's own version option: -v as the short form (clap offers -V), --version as the long one.
    #[arg(short = 'v', long, action = clap::ArgAction::Version)]
    version: (),

    /// Listening address of the web server, for this launch. It must be
    /// local (127.0.0.1 or ::1). Default: 127.0.0.1 and the saved port (8484
    /// at first).
    #[arg(long)]
    pub listen: Option<SocketAddr>,

    /// Data folder (internal database and master key).
    /// Default: %APPDATA%\Prisme on Windows, ~/.local/share/Prisme on Linux.
    #[arg(long)]
    pub data_dir: Option<PathBuf>,

    /// Email address of the user, for this launch (created if it does not
    /// exist). Default: the saved user.
    #[arg(long)]
    pub user_email: Option<String>,

    /// Asks the first-launch questions again (port and email address) and
    /// saves the new answers.
    #[arg(long)]
    pub setup: bool,
}

impl Config {
    /// Checks the consistency of the options: without authentication,
    /// listening must be local; the email address must be well formed.
    pub fn validate(&self) -> anyhow::Result<()> {
        if let Some(listen) = self.listen
            && !listen.ip().is_loopback()
        {
            bail!(
                "the address {listen} is reachable from the network: without authentication, \
                 Prisme only listens on 127.0.0.1 or ::1"
            );
        }
        if let Some(email) = &self.user_email {
            crate::setup::parse_email(email).map_err(anyhow::Error::msg)?;
        }
        Ok(())
    }

    /// Returns the data folder: that of `--data-dir`, otherwise the default folder.
    pub fn data_dir(&self) -> anyhow::Result<PathBuf> {
        match &self.data_dir {
            Some(dir) => Ok(dir.clone()),
            None => dirs::data_dir()
                .map(|dir| dir.join(DATA_DIR_NAME))
                .context("user data folder not found: specify --data-dir"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_listening_on(listen: &str) -> Config {
        Config::parse_from(["prisme", "--listen", listen])
    }

    #[test]
    fn accepts_loopback_addresses() {
        assert!(config_listening_on("127.0.0.1:8484").validate().is_ok());
        assert!(config_listening_on("[::1]:8484").validate().is_ok());
    }

    #[test]
    fn rejects_network_addresses() {
        assert!(config_listening_on("0.0.0.0:8484").validate().is_err());
        assert!(config_listening_on("192.168.1.10:8484").validate().is_err());
    }

    #[test]
    fn version_is_v_or_version() {
        use clap::error::ErrorKind;

        for flag in ["-v", "--version"] {
            let error = Config::try_parse_from(["prisme", flag]).unwrap_err();
            assert_eq!(error.kind(), ErrorKind::DisplayVersion);
        }
        let error = Config::try_parse_from(["prisme", "-V"]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::UnknownArgument);
    }

    #[test]
    fn explicit_data_dir_wins() {
        let config = Config::parse_from(["prisme", "--data-dir", "D:/prisme-data"]);
        assert_eq!(config.data_dir().unwrap(), PathBuf::from("D:/prisme-data"));
    }
}
