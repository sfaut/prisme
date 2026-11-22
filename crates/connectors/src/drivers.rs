//! Registry of the drivers: the driver of each engine. The only place that
//! names every engine; the server asks it for the driver of a connection,
//! then only uses the contracts of `api`.

use crate::api::{Driver, Engine};
use crate::mysql;

/// Driver of the engine `engine`.
pub fn of(engine: Engine) -> &'static dyn Driver {
    match engine {
        Engine::Mysql => &mysql::DRIVER,
    }
}
