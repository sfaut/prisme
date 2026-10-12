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

/// Options de lancement de Prisme.
#[derive(Debug, Parser)]
#[command(
    name = "prisme",
    version,
    about = "Client web multi-bases de données en notebook"
)]
pub struct Config {
    /// Adresse d'écoute du serveur web. Elle doit être locale (127.0.0.1 ou ::1).
    #[arg(long, default_value = "127.0.0.1:8484")]
    pub listen: SocketAddr,

    /// Dossier de données (base interne et clef maîtresse).
    /// Par défaut : %APPDATA%\Prisme sous Windows, ~/.local/share/Prisme sous Linux.
    #[arg(long)]
    pub data_dir: Option<PathBuf>,

    /// Adresse e-mail de l'utilisateur unique, créé au premier lancement.
    #[arg(long, default_value = "sebastien.faut@gmail.com")]
    pub user_email: String,
}

impl Config {
    /// Vérifie la cohérence des options : sans authentification, l'écoute doit être locale.
    pub fn validate(&self) -> anyhow::Result<()> {
        if !self.listen.ip().is_loopback() {
            bail!(
                "l'adresse {} est accessible depuis le réseau : sans authentification, \
                 Prisme n'écoute que sur 127.0.0.1 ou ::1",
                self.listen
            );
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
    fn explicit_data_dir_wins() {
        let config = Config::parse_from(["prisme", "--data-dir", "D:/prisme-data"]);
        assert_eq!(config.data_dir().unwrap(), PathBuf::from("D:/prisme-data"));
    }
}
