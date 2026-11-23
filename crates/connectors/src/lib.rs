//! Prisme's connectors: access to each target database engine.
//!
//! One module per family of engines, each with its native driver (see
//! `docs/V1.md`, section 3.2). For now: MySQL and MariaDB (`mysql`), and
//! SQLite (`sqlite`). What every
//! engine gives takes a neutral form, that each connector fills: the
//! flavor of its source (`flavor`), the columns and cells of a result
//! (`columns`), the outcome of a statement (`outcome`), its failures
//! (`failure`) and the trace of the statements sent (`sql_log`); numbers
//! are written alike for every engine (`number_text`). The databases held in
//! files are reached through the providers of their locations (`location`).
//! The engines whose driver blocks run each connection on a thread of its
//! own (`blocking`).

pub mod api;
pub mod blocking;
pub mod catalog;
pub mod columns;
pub mod drivers;
pub mod failure;
pub mod flavor;
pub mod location;
pub mod mysql;
pub mod number_text;
pub mod outcome;
mod secret_mask;
pub mod sql_log;
pub mod sqlite;
#[cfg(feature = "test-servers")]
pub mod test_servers;
