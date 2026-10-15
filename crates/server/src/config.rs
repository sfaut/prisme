//! Configuration de Prisme, lue sur la ligne de commande.
//!
//! La v1 ne connaît que le mode poste de développement : un seul utilisateur,
//! sans authentification, donc une écoute limitée à la machine locale pour
//! qu'aucune autre machine du réseau n'accède aux connexions enregistrées.

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, bail};
use clap::Parser;

/// Nom du dossier de données créé dans le dossier de données de l'utilisateur.
const DATA_DIR_NAME: &str = "Prisme";

/// Fonctionnement du premier lancement, affiché à la fin de `prisme --help`.
const AFTER_HELP: &str = "\
Premier lancement :
  Quand la base interne n'existe pas encore dans le dossier de données,
  Prisme demande dans le terminal le port d'écoute (8484 par défaut, sur
  127.0.0.1) et l'adresse e-mail de l'utilisateur (me@local par défaut).
  Les réponses sont enregistrées dans la base interne et relues aux
  lancements suivants ; --setup repose les questions.
  Une option --listen ou --user-email remplace sa question, sans être
  enregistrée. Sans terminal interactif, les valeurs par défaut s'appliquent.";

/// Options de lancement de Prisme.
#[derive(Debug, Parser)]
#[command(
    name = "prisme",
    version,
    about = "Client web multi-bases de données en notebook",
    after_help = AFTER_HELP,
    disable_version_flag = true
)]
pub struct Config {
    /// Print version
    // Option de version propre à Prisme : -v en court (clap propose -V), --version en long.
    #[arg(short = 'v', long, action = clap::ArgAction::Version)]
    version: (),

    /// Adresse d'écoute du serveur web, pour ce lancement. Elle doit être
    /// locale (127.0.0.1 ou ::1). Par défaut : 127.0.0.1 et le port
    /// enregistré (8484 au départ).
    #[arg(long)]
    pub listen: Option<SocketAddr>,

    /// Dossier de données (base interne et clef maîtresse).
    /// Par défaut : %APPDATA%\Prisme sous Windows, ~/.local/share/Prisme sous Linux.
    #[arg(long)]
    pub data_dir: Option<PathBuf>,

    /// Adresse e-mail de l'utilisateur, pour ce lancement (créé s'il
    /// n'existe pas). Par défaut : l'utilisateur enregistré.
    #[arg(long)]
    pub user_email: Option<String>,

    /// Repose les questions du premier lancement (port et adresse e-mail)
    /// et enregistre les nouvelles réponses.
    #[arg(long)]
    pub setup: bool,
}

impl Config {
    /// Vérifie la cohérence des options : sans authentification, l'écoute
    /// doit être locale ; l'adresse e-mail doit être bien formée.
    pub fn validate(&self) -> anyhow::Result<()> {
        if let Some(listen) = self.listen
            && !listen.ip().is_loopback()
        {
            bail!(
                "l'adresse {listen} est accessible depuis le réseau : sans authentification, \
                 Prisme n'écoute que sur 127.0.0.1 ou ::1"
            );
        }
        if let Some(email) = &self.user_email {
            crate::setup::parse_email(email).map_err(anyhow::Error::msg)?;
        }
        Ok(())
    }

    /// Renvoie le dossier de données : celui de `--data-dir`, sinon le dossier par défaut.
    pub fn data_dir(&self) -> anyhow::Result<PathBuf> {
        match &self.data_dir {
            Some(dir) => Ok(dir.clone()),
            None => dirs::data_dir()
                .map(|dir| dir.join(DATA_DIR_NAME))
                .context("dossier de données de l'utilisateur introuvable : préciser --data-dir"),
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
