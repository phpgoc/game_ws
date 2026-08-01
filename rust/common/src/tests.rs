#[cfg(debug_assertions)]
use super::{__dlog, level_color};
#[cfg(debug_assertions)]
use tracing::Level;

#[cfg(debug_assertions)]
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

#[cfg(not(debug_assertions))]
#[test]
fn release_logging_does_not_evaluate_arguments() {
    use std::cell::Cell;

    let formatted_argument_evaluated = Cell::new(false);
    crate::dlog!(crate::tracing::Level::DEBUG, "{}", {
        formatted_argument_evaluated.set(true);
        "formatted log"
    });

    let message_evaluated = Cell::new(false);
    crate::dlog!(
        {
            message_evaluated.set(true);
            "message log"
        },
        crate::tracing::Level::TRACE
    );

    assert!(!formatted_argument_evaluated.get());
    assert!(!message_evaluated.get());
}
