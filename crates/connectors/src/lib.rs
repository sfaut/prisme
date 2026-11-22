//! Prisme's connectors: access to each target database engine.
//!
//! One module per family of engines, each with its native driver (see
//! `docs/V1.md`, section 3.2). For now: MySQL and MariaDB. What every
//! engine gives takes a neutral form, that each connector fills: the
//! flavor of its source (`flavor`), the columns and cells of a result
//! (`columns`), the outcome of a statement (`outcome`), its failures
//! (`failure`) and the trace of the statements sent (`sql_log`); numbers
//! are written alike for every engine (`number_text`).

pub mod columns;
pub mod failure;
pub mod flavor;
pub mod mysql;
pub mod number_text;
pub mod outcome;
mod secret_mask;
pub mod sql_log;
#[cfg(feature = "test-servers")]
pub mod test_servers;
