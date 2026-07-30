use super::{__dlog, level_color};
use tracing::Level;

#[test]
fn debug_logging_supports_every_level_and_macro_form() {
    assert_eq!(level_color(Level::ERROR), "\x1b[31m");
    assert_eq!(level_color(Level::WARN), "\x1b[33m");
    assert_eq!(level_color(Level::INFO), "\x1b[32m");
    assert_eq!(level_color(Level::DEBUG), "\x1b[36m");
    assert_eq!(level_color(Level::TRACE), "\x1b[90m");

    __dlog("direct log", Level::INFO, "tests.rs", 1);
    crate::dlog!(Level::DEBUG, "formatted {}", "log");
    crate::dlog!("message log", Level::TRACE);
}
