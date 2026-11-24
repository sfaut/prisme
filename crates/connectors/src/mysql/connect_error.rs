//! Connection failures to a MySQL or MariaDB server, and what the driver or
//! the server said about them.
//!
//! The driver returns technical errors ("Input/output error: os error
//! 10061", "Server error: ERROR 1045..."). `describe` turns them into a
//! `ConnectFailure` that says what is wrong and what to check, and
//! `FailureCause::from` sorts them by the author of their text
//! (`crate::failure`).

use std::io::ErrorKind;
use std::time::Duration;

use super::MysqlTarget;
use crate::failure::{ConnectFailure, FailureCause};
use mysql_async::{DriverError, IoError};

/// MySQL error code: access denied (password, or account unknown for this host).
const ER_ACCESS_DENIED: u16 = 1045;

/// MySQL error code: the account has no access to the requested database.
const ER_DB_ACCESS_DENIED: u16 = 1044;

/// MySQL error code: unknown database.
const ER_BAD_DB: u16 = 1049;

/// MySQL error code: connection refused for this host.
const ER_HOST_NOT_PRIVILEGED: u16 = 1130;

impl From<&mysql_async::Error> for FailureCause {
    /// Sorts a driver error by the author of its text.
    fn from(error: &mysql_async::Error) -> Self {
        match error {
            mysql_async::Error::Server(server) => Self::Server {
                number: server.code,
                sqlstate: server.state.clone(),
                message: server.message.clone(),
            },
            mysql_async::Error::Io(IoError::Io(io)) => Self::System {
                message: io.to_string(),
            },
            _ => Self::Driver {
                message: error.to_string(),
            },
        }
    }
}

/// Failure when the server `target` does not answer within `timeout`.
pub fn timeout(target: &MysqlTarget, timeout: Duration) -> ConnectFailure {
    ConnectFailure::Timeout {
        address: address(target),
        seconds: timeout.as_secs(),
    }
}

/// Failure of a connection to `target`, reported by the driver.
pub fn describe(target: &MysqlTarget, error: &mysql_async::Error) -> ConnectFailure {
    let address = address(target);
    let cause = FailureCause::from(error);
    match error {
        mysql_async::Error::Io(IoError::Io(io)) => match io.kind() {
            ErrorKind::ConnectionRefused => ConnectFailure::Refused { address },
            ErrorKind::TimedOut | ErrorKind::HostUnreachable | ErrorKind::NetworkUnreachable => {
                ConnectFailure::Unreachable { address }
            }
            _ => ConnectFailure::Other { address, cause },
        },
        mysql_async::Error::Server(server) => match server.code {
            ER_ACCESS_DENIED => ConnectFailure::AccessDenied {
                username: target.username.clone(),
                cause,
            },
            ER_HOST_NOT_PRIVILEGED => ConnectFailure::HostNotAllowed {
                username: target.username.clone(),
                cause,
            },
            ER_BAD_DB => ConnectFailure::UnknownDatabase {
                database: target.database_name.clone(),
                address,
            },
            ER_DB_ACCESS_DENIED => ConnectFailure::DatabaseAccessDenied {
                username: target.username.clone(),
                database: target.database_name.clone(),
            },
            _ => ConnectFailure::ServerRefused { address, cause },
        },
        mysql_async::Error::Driver(DriverError::UnknownAuthPlugin { name }) => {
            ConnectFailure::UnsupportedAuthentication {
                username: target.username.clone(),
                plugin: name.clone(),
            }
        }
        _ => ConnectFailure::Other { address, cause },
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
        let address = "192.168.0.4:3307".to_string();
        assert_eq!(
            describe(&target(), &io_error(ErrorKind::ConnectionRefused)),
            ConnectFailure::Refused {
                address: address.clone()
            }
        );
        assert_eq!(
            describe(&target(), &io_error(ErrorKind::TimedOut)),
            ConnectFailure::Unreachable {
                address: address.clone()
            }
        );
        let failure = timeout(&target(), Duration::from_secs(10));
        assert_eq!(
            failure,
            ConnectFailure::Timeout {
                address,
                seconds: 10
            }
        );
        assert!(
            failure
                .to_string()
                .starts_with("192.168.0.4:3307 does not answer (10 s timeout exceeded)")
        );
    }

    #[test]
    fn explains_server_refusals() {
        let denied = describe(&target(), &server_error(ER_ACCESS_DENIED));
        assert_eq!(
            denied,
            ConnectFailure::AccessDenied {
                username: "root".to_string(),
                cause: FailureCause::Server {
                    number: ER_ACCESS_DENIED,
                    sqlstate: "28000".to_string(),
                    message: "server message".to_string(),
                },
            }
        );
        assert!(
            denied
                .to_string()
                .starts_with("access denied for the account \"root\"")
        );
        assert!(
            describe(&target(), &server_error(ER_HOST_NOT_PRIVILEGED))
                .to_string()
                .starts_with("the server refuses connections from this machine")
        );
        assert_eq!(
            describe(&target(), &server_error(ER_BAD_DB)).to_string(),
            "the database \"shop\" does not exist on 192.168.0.4:3307"
        );
    }

    #[test]
    fn sorts_the_causes_by_author() {
        assert!(matches!(
            FailureCause::from(&io_error(ErrorKind::BrokenPipe)),
            FailureCause::System { .. }
        ));
        assert!(matches!(
            FailureCause::from(&mysql_async::Error::Driver(DriverError::ConnectionClosed)),
            FailureCause::Driver { .. }
        ));
    }
}
