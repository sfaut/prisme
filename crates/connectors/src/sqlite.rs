//! Connector of SQLite, the library embedded in Prisme (`rusqlite`).
//!
//! A source of SQLite is a main database and the databases attached to it,
//! each at a location (`Settings::Files`): a file, a remote file copied into
//! the cache, or a database in memory. A session is one connection, held by
//! a thread of its own (`blocking`), as the library blocks: it opens the
//! main database without ever creating a file, attaches the others (a
//! database that cannot be opened is left out, and its state tells why),
//! then runs the statements of the target. A read-only session opens its
//! connection read-only, which its attachments inherit; a remote copy is
//! attached read-only in any case.
//!
//! The databases in memory live in the `memdb` file system of SQLite, named
//! by the key of the session: the connections of one session (its parallel
//! queries) share them, and they end with the last of these connections.
//!
//! Each module holds one part: the opening of a connection (`open`), the
//! statements and their rows (`statement`), the catalog (`catalog`), the
//! errors of the library (`failure`), and the driver behind the contracts of
//! `api` (`driver`). The behaviours of the library it relies on are
//! measured by `bench/sqlite` (`docs/SQL-COMPARISON.md`, section 6).

mod catalog;
mod driver;
mod failure;
mod open;
mod statement;

pub use driver::{DRIVER, SqliteDriver, capabilities};
