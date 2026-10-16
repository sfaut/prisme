//! Prisme's connectors: access to each target database engine.
//!
//! One module per family of engines, each with its native driver (see
//! `docs/V1.md`, section 3.2). For now: MySQL and MariaDB.

pub mod mysql;
