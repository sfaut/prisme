//! Contract suite of the connectors: the same tests, written once against
//! the traits of `prisme_connectors::api`, run on every engine. An engine
//! joins the suite with its `Fixture` (a module of this folder) and a line
//! of `contract_tests!` below (`docs/DEVELOPER.md`, section "Adding an
//! engine").
//!
//! The MySQL and MariaDB tests need the development servers, so they are
//! ignored by default: `cargo test -p prisme-connectors -- --include-ignored`
//! after `docker compose up --detach --wait`.

mod mysql;
mod suite;

/// Declares, in the module `$module`, one test per test of the suite, run on
/// each fixture of `$fixtures`, with the attributes `$attr` (`#[ignore =
/// "..."]` for an engine that needs a server).
macro_rules! contract_tests {
    ($module:ident, $fixtures:expr, [$(#[$attr:meta])*]) => {
        mod $module {
            contract_tests!(@tests $fixtures, [$(#[$attr])*],
                probes_the_source,
                streams_rows_and_reports_writes,
                fails_a_statement_and_stays_usable,
                follows_the_transaction,
                interrupts_a_statement_and_stays_usable,
                refuses_writes_when_read_only,
                opens_on_a_namespace,
                switches_the_namespace,
                reads_the_catalog,
            );
        }
    };
    (@tests $fixtures:expr, $attrs:tt, $($test:ident),* $(,)?) => {
        $(contract_tests!(@test $fixtures, $attrs, $test);)*
    };
    (@test $fixtures:expr, [$(#[$attr:meta])*], $test:ident) => {
        #[tokio::test]
        $(#[$attr])*
        async fn $test() {
            for fixture in $fixtures {
                super::suite::$test(&fixture).await;
            }
        }
    };
}

contract_tests!(
    on_mysql,
    super::mysql::fixtures(),
    [#[ignore = "requires the development servers (docker compose up --detach --wait)"]]
);
