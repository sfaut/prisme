//! Errors of SQLite in the neutral forms of `crate::failure`: an error of the
//! library is an answer of the engine (`FailureCause::Engine`), with its
//! extended code and the name of that code; the other errors of `rusqlite`
//! (a value it cannot convert, a statement it refuses) are the driver's. An
//! error while a database opens tells what is wrong with its file
//! (`ConnectFailure`).

use rusqlite::ffi;

use crate::api::ConnectorError;
use crate::failure::{ConnectFailure, FailureCause};
use crate::location::Location;

impl From<rusqlite::Error> for ConnectorError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Query(cause(&error))
    }
}

/// Text that explains an error of `rusqlite`: the answer of the library, or
/// the message of the driver.
pub(super) fn cause(error: &rusqlite::Error) -> FailureCause {
    match error {
        rusqlite::Error::SqliteFailure(failure, message) => FailureCause::Engine {
            code: failure.extended_code.to_string(),
            name: code_name(failure.extended_code).to_string(),
            message: message.clone().unwrap_or_else(|| failure.to_string()),
        },
        other => FailureCause::Driver {
            message: other.to_string(),
        },
    }
}

/// Why the database at `location` could not be opened, from the error of
/// the library: not a database, or a file it cannot open.
pub(super) fn opening(location: &Location, error: &rusqlite::Error) -> ConnectFailure {
    let location = location.input().to_string();
    let cause = cause(error);
    match error.sqlite_error_code() {
        Some(ffi::ErrorCode::NotADatabase) => ConnectFailure::NotADatabase { location, cause },
        _ => ConnectFailure::FileUnreadable { location, cause },
    }
}

/// Did the statement stop on an interruption (`sqlite3_interrupt`)?
pub(super) fn is_interruption(error: &rusqlite::Error) -> bool {
    error.sqlite_error_code() == Some(ffi::ErrorCode::OperationInterrupted)
}

/// Name of a result code of the library (`SQLITE_CONSTRAINT_UNIQUE`): that
/// of the extended code, else that of its primary code (its low byte), as
/// `sqlite3.h` writes them.
fn code_name(code: i32) -> &'static str {
    name_of(code)
        .or_else(|| name_of(code & 0xff))
        .unwrap_or("SQLITE_UNKNOWN")
}

/// Matches a code against the constants of the library, named as written.
macro_rules! code_names {
    ($code:expr, $($name:ident),* $(,)?) => {
        match $code {
            $(ffi::$name => Some(stringify!($name)),)*
            _ => None,
        }
    };
}

/// Name of the result code `code`, if the library defines it. The list is
/// that of `sqlite3.h` (primary codes, then extended codes), without the
/// codes of a success (`SQLITE_OK`, `SQLITE_ROW`, `SQLITE_DONE`).
fn name_of(code: i32) -> Option<&'static str> {
    code_names!(
        code,
        SQLITE_ERROR,
        SQLITE_INTERNAL,
        SQLITE_PERM,
        SQLITE_ABORT,
        SQLITE_BUSY,
        SQLITE_LOCKED,
        SQLITE_NOMEM,
        SQLITE_READONLY,
        SQLITE_INTERRUPT,
        SQLITE_IOERR,
        SQLITE_CORRUPT,
        SQLITE_NOTFOUND,
        SQLITE_FULL,
        SQLITE_CANTOPEN,
        SQLITE_PROTOCOL,
        SQLITE_EMPTY,
        SQLITE_SCHEMA,
        SQLITE_TOOBIG,
        SQLITE_CONSTRAINT,
        SQLITE_MISMATCH,
        SQLITE_MISUSE,
        SQLITE_NOLFS,
        SQLITE_AUTH,
        SQLITE_FORMAT,
        SQLITE_RANGE,
        SQLITE_NOTADB,
        SQLITE_NOTICE,
        SQLITE_WARNING,
        SQLITE_ERROR_MISSING_COLLSEQ,
        SQLITE_ERROR_RETRY,
        SQLITE_ERROR_SNAPSHOT,
        SQLITE_ERROR_RESERVESIZE,
        SQLITE_ERROR_KEY,
        SQLITE_ERROR_UNABLE,
        SQLITE_IOERR_READ,
        SQLITE_IOERR_SHORT_READ,
        SQLITE_IOERR_WRITE,
        SQLITE_IOERR_FSYNC,
        SQLITE_IOERR_DIR_FSYNC,
        SQLITE_IOERR_TRUNCATE,
        SQLITE_IOERR_FSTAT,
        SQLITE_IOERR_UNLOCK,
        SQLITE_IOERR_RDLOCK,
        SQLITE_IOERR_DELETE,
        SQLITE_IOERR_BLOCKED,
        SQLITE_IOERR_NOMEM,
        SQLITE_IOERR_ACCESS,
        SQLITE_IOERR_CHECKRESERVEDLOCK,
        SQLITE_IOERR_LOCK,
        SQLITE_IOERR_CLOSE,
        SQLITE_IOERR_DIR_CLOSE,
        SQLITE_IOERR_SHMOPEN,
        SQLITE_IOERR_SHMSIZE,
        SQLITE_IOERR_SHMLOCK,
        SQLITE_IOERR_SHMMAP,
        SQLITE_IOERR_SEEK,
        SQLITE_IOERR_DELETE_NOENT,
        SQLITE_IOERR_MMAP,
        SQLITE_IOERR_GETTEMPPATH,
        SQLITE_IOERR_CONVPATH,
        SQLITE_IOERR_VNODE,
        SQLITE_IOERR_AUTH,
        SQLITE_IOERR_BEGIN_ATOMIC,
        SQLITE_IOERR_COMMIT_ATOMIC,
        SQLITE_IOERR_ROLLBACK_ATOMIC,
        SQLITE_IOERR_DATA,
        SQLITE_IOERR_CORRUPTFS,
        SQLITE_IOERR_IN_PAGE,
        SQLITE_IOERR_BADKEY,
        SQLITE_IOERR_CODEC,
        SQLITE_LOCKED_SHAREDCACHE,
        SQLITE_LOCKED_VTAB,
        SQLITE_BUSY_RECOVERY,
        SQLITE_BUSY_SNAPSHOT,
        SQLITE_BUSY_TIMEOUT,
        SQLITE_CANTOPEN_NOTEMPDIR,
        SQLITE_CANTOPEN_ISDIR,
        SQLITE_CANTOPEN_FULLPATH,
        SQLITE_CANTOPEN_CONVPATH,
        SQLITE_CANTOPEN_DIRTYWAL,
        SQLITE_CANTOPEN_SYMLINK,
        SQLITE_CORRUPT_VTAB,
        SQLITE_CORRUPT_SEQUENCE,
        SQLITE_CORRUPT_INDEX,
        SQLITE_READONLY_RECOVERY,
        SQLITE_READONLY_CANTLOCK,
        SQLITE_READONLY_ROLLBACK,
        SQLITE_READONLY_DBMOVED,
        SQLITE_READONLY_CANTINIT,
        SQLITE_READONLY_DIRECTORY,
        SQLITE_ABORT_ROLLBACK,
        SQLITE_CONSTRAINT_CHECK,
        SQLITE_CONSTRAINT_COMMITHOOK,
        SQLITE_CONSTRAINT_FOREIGNKEY,
        SQLITE_CONSTRAINT_FUNCTION,
        SQLITE_CONSTRAINT_NOTNULL,
        SQLITE_CONSTRAINT_PRIMARYKEY,
        SQLITE_CONSTRAINT_TRIGGER,
        SQLITE_CONSTRAINT_UNIQUE,
        SQLITE_CONSTRAINT_VTAB,
        SQLITE_CONSTRAINT_ROWID,
        SQLITE_CONSTRAINT_PINNED,
        SQLITE_CONSTRAINT_DATATYPE,
        SQLITE_NOTICE_RECOVER_WAL,
        SQLITE_NOTICE_RECOVER_ROLLBACK,
        SQLITE_NOTICE_RBU,
        SQLITE_WARNING_AUTOINDEX,
        SQLITE_AUTH_USER,
    )
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::*;

    #[test]
    fn names_the_extended_code_of_an_answer() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("CREATE TABLE t (k TEXT UNIQUE); INSERT INTO t VALUES ('a')")
            .unwrap();
        let error = connection
            .execute("INSERT INTO t VALUES ('a')", [])
            .unwrap_err();
        assert_eq!(
            cause(&error),
            FailureCause::Engine {
                code: "2067".to_string(),
                name: "SQLITE_CONSTRAINT_UNIQUE".to_string(),
                message: "UNIQUE constraint failed: t.k".to_string(),
            }
        );
        let missing = connection.prepare("SELECT * FROM missing").unwrap_err();
        assert_eq!(
            cause(&missing).to_string(),
            "SQLITE_ERROR (1): no such table: missing"
        );
    }

    #[test]
    fn names_a_code_by_its_primary_code_when_unknown() {
        assert_eq!(code_name(ffi::SQLITE_BUSY | (99 << 8)), "SQLITE_BUSY");
        assert_eq!(code_name(1000), "SQLITE_UNKNOWN");
    }
}
