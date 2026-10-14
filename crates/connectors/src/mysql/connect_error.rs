//! Messages d'échec de connexion à un serveur MySQL ou MariaDB.
//!
//! Le pilote renvoie des erreurs techniques (« Input/output error: os error
//! 10061 », « Server error: ERROR 1045... »). Ce module les traduit en un
//! message qui dit ce qui ne va pas et quoi vérifier. Le message du serveur,
//! quand il existe, reste cité à la fin pour le détail.

use std::io::ErrorKind;
use std::time::Duration;

use mysql_async::{DriverError, IoError};

use super::MysqlTarget;

/// Code d'erreur MySQL : accès refusé (mot de passe, ou compte inconnu pour cet hôte).
const ER_ACCESS_DENIED: u16 = 1045;

/// Code d'erreur MySQL : le compte n'a pas accès à la base demandée.
const ER_DB_ACCESS_DENIED: u16 = 1044;

/// Code d'erreur MySQL : base inconnue.
const ER_BAD_DB: u16 = 1049;

/// Code d'erreur MySQL : connexion refusée pour cet hôte.
const ER_HOST_NOT_PRIVILEGED: u16 = 1130;

/// Message quand le serveur `target` ne répond pas dans le délai `timeout`.
pub fn timeout_message(target: &MysqlTarget, timeout: Duration) -> String {
    format!(
        "{} ne répond pas (délai de {} s dépassé) : serveur éteint, adresse ou port erronés, ou pare-feu",
        address(target),
        timeout.as_secs()
    )
}

/// Message d'un échec de connexion à `target`, signalé par le pilote.
pub fn describe(target: &MysqlTarget, error: &mysql_async::Error) -> String {
    let address = address(target);
    match error {
        mysql_async::Error::Io(IoError::Io(io)) => match io.kind() {
            ErrorKind::ConnectionRefused => format!(
                "{address} refuse la connexion : aucun serveur n'écoute sur ce port, \
                 ou il n'accepte que les connexions locales (bind-address)"
            ),
            ErrorKind::TimedOut | ErrorKind::HostUnreachable | ErrorKind::NetworkUnreachable => {
                format!("{address} est injoignable : serveur éteint, adresse erronée ou pare-feu")
            }
            _ => format!("connexion à {address} impossible : {io}"),
        },
        mysql_async::Error::Server(server) => match server.code {
            ER_ACCESS_DENIED => format!(
                "accès refusé pour le compte « {} » : mot de passe erroné, ou compte non \
                 autorisé depuis ce poste (le serveur répond : {})",
                target.username, server.message
            ),
            ER_HOST_NOT_PRIVILEGED => format!(
                "le serveur refuse les connexions de ce poste : aucun compte « {} » n'y est \
                 autorisé depuis cette adresse (le serveur répond : {})",
                target.username, server.message
            ),
            ER_BAD_DB => format!(
                "la base « {} » n'existe pas sur {address}",
                target.database_name
            ),
            ER_DB_ACCESS_DENIED => format!(
                "le compte « {} » n'a pas accès à la base « {} »",
                target.username, target.database_name
            ),
            _ => format!("{address} refuse la connexion : {}", server.message),
        },
        mysql_async::Error::Driver(DriverError::UnknownAuthPlugin { name }) => format!(
            "le compte « {} » utilise une méthode d'authentification non prise en charge ({name})",
            target.username
        ),
        _ => format!("connexion à {address} impossible : {error}"),
    }
}

/// Adresse du serveur, sous la forme `hôte:port`.
fn address(target: &MysqlTarget) -> String {
    format!("{}:{}", target.host, target.port)
}

#[cfg(test)]
mod tests {
    use std::io;

    use mysql_async::ServerError;
    use secrecy::SecretString;

    use super::*;

    fn target() -> MysqlTarget {
        MysqlTarget {
            host: "192.168.0.4".to_string(),
            port: 3307,
            username: "root".to_string(),
            password: SecretString::from("x"),
            database_name: "shop".to_string(),
            init_statements: Vec::new(),
        }
    }

    fn io_error(kind: ErrorKind) -> mysql_async::Error {
        mysql_async::Error::Io(IoError::Io(io::Error::from(kind)))
    }

    fn server_error(code: u16) -> mysql_async::Error {
        mysql_async::Error::Server(ServerError {
            code,
            message: "message du serveur".to_string(),
            state: "28000".to_string(),
        })
    }

    #[test]
    fn explains_network_failures() {
        assert!(
            describe(&target(), &io_error(ErrorKind::ConnectionRefused))
                .starts_with("192.168.0.4:3307 refuse la connexion")
        );
        assert!(
            describe(&target(), &io_error(ErrorKind::TimedOut))
                .starts_with("192.168.0.4:3307 est injoignable")
        );
        assert!(
            timeout_message(&target(), Duration::from_secs(10))
                .starts_with("192.168.0.4:3307 ne répond pas (délai de 10 s dépassé)")
        );
    }

    #[test]
    fn explains_server_refusals() {
        let denied = describe(&target(), &server_error(ER_ACCESS_DENIED));
        assert!(denied.starts_with("accès refusé pour le compte « root »"));
        assert!(denied.ends_with("(le serveur répond : message du serveur)"));
        assert!(
            describe(&target(), &server_error(ER_HOST_NOT_PRIVILEGED))
                .starts_with("le serveur refuse les connexions de ce poste")
        );
        assert_eq!(
            describe(&target(), &server_error(ER_BAD_DB)),
            "la base « shop » n'existe pas sur 192.168.0.4:3307"
        );
    }
}
