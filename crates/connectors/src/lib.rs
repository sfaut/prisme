//! Prisme's connectors: access to each target database engine.
//!
//! One module per family of engines, each with its native driver (see
//! `docs/V1.md`, section 3.2). For now: MySQL and MariaDB. What the
//! engines report about a statement takes a neutral form (`outcome`), that
//! each connector fills; numbers are written alike for every engine
//! (`number_text`).

pub mod mysql;
pub mod number_text;
pub mod outcome;
#[cfg(feature = "test-servers")]
pub mod test_servers;
