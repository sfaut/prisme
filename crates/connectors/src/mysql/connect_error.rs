//! Messages for connection failures to a MySQL or MariaDB server.
//!
//! The driver returns technical errors ("Input/output error: os error
//! 10061", "Server error: ERROR 1045..."). This module turns them into a
//! message that says what is wrong and what to check. The server's message,
//! when there is one, stays quoted at the end for detail.

use std::io::ErrorKind;
use std::time::Duration;

use mysql_async::{DriverError, IoError};

use super::MysqlTarget;

/// MySQL error code: access denied (password, or account unknown for this host).
const ER_ACCESS_DENIED: u16 = 1045;

/// MySQL error code: the account has no access to the requested database.
const ER_DB_ACCESS_DENIED: u16 = 1044;

/// MySQL error code: unknown database.
const ER_BAD_DB: u16 = 1049;

/// MySQL error code: connection refused for this host.
const ER_HOST_NOT_PRIVILEGED: u16 = 1130;

/// Message when the server `target` does not answer within `timeout`.
pub fn timeout_message(target: &MysqlTarget, timeout: Duration) -> String {
    format!(
        "{} does not answer ({} s timeout exceeded): server down, wrong address or port, or firewall",
        address(target),
        timeout.as_secs()
    )
}

/// Message for a connection failure to `target`, reported by the driver.
pub fn describe(target: &MysqlTarget, error: &mysql_async::Error) -> String {
    let address = address(target);
    match error {
        mysql_async::Error::Io(IoError::Io(io)) => match io.kind() {
            ErrorKind::ConnectionRefused => format!(
                "{address} refuses the connection: no server listens on this port, \
                 or it only accepts local connections (bind-address)"
            ),
            ErrorKind::TimedOut | ErrorKind::HostUnreachable | ErrorKind::NetworkUnreachable => {
                format!("{address} is unreachable: server down, wrong address or firewall")
            }
            _ => format!("cannot connect to {address}: {io}"),
        },
        mysql_async::Error::Server(server) => match server.code {
            ER_ACCESS_DENIED => format!(
                "access denied for the account \"{}\": wrong password, or account not \
                 allowed from this machine (the server answers: {})",
                target.username, server.message
            ),
            ER_HOST_NOT_PRIVILEGED => format!(
                "the server refuses connections from this machine: no account \"{}\" is \
                 allowed from this address (the server answers: {})",
                target.username, server.message
            ),
            ER_BAD_DB => format!(
                "the database \"{}\" does not exist on {address}",
                target.database_name
            ),
            ER_DB_ACCESS_DENIED => format!(
                "the account \"{}\" has no access to the database \"{}\"",
                target.username, target.database_name
            ),
            _ => format!("{address} refuses the connection: {}", server.message),
        },
        mysql_async::Error::Driver(DriverError::UnknownAuthPlugin { name }) => format!(
            "the account \"{}\" uses an unsupported authentication method ({name})",
            target.username
        ),
        _ => format!("cannot connect to {address}: {error}"),
    }
}

/// Address of the server, as `host:port`.
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
            message: "server message".to_string(),
            state: "28000".to_string(),
        })
    }

    #[test]
    fn explains_network_failures() {
        assert!(
            describe(&target(), &io_error(ErrorKind::ConnectionRefused))
                .starts_with("192.168.0.4:3307 refuses the connection")
        );
        assert!(
            describe(&target(), &io_error(ErrorKind::TimedOut))
                .starts_with("192.168.0.4:3307 is unreachable")
        );
        assert!(
            timeout_message(&target(), Duration::from_secs(10))
                .starts_with("192.168.0.4:3307 does not answer (10 s timeout exceeded)")
        );
    }

    #[test]
    fn explains_server_refusals() {
        let denied = describe(&target(), &server_error(ER_ACCESS_DENIED));
        assert!(denied.starts_with("access denied for the account \"root\""));
        assert!(denied.ends_with("(the server answers: server message)"));
        assert!(
            describe(&target(), &server_error(ER_HOST_NOT_PRIVILEGED))
                .starts_with("the server refuses connections from this machine")
        );
        assert_eq!(
            describe(&target(), &server_error(ER_BAD_DB)),
            "the database \"shop\" does not exist on 192.168.0.4:3307"
        );
    }
}
