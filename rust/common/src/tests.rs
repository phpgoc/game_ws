use std::{
    io::Write,
    sync::{Arc, Mutex},
};

use tracing::Level;

#[derive(Clone)]
struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

impl Write for SharedBuffer {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("lock log buffer").extend(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn logging_supports_the_three_service_levels_and_both_macro_forms() {
    let output = Arc::new(Mutex::new(Vec::new()));
    let make_writer = {
        let output = Arc::clone(&output);
        move || SharedBuffer(Arc::clone(&output))
    };
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_max_level(Level::DEBUG)
        .with_writer(make_writer)
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        crate::dlog!(Level::ERROR, "error log");
        crate::dlog!(Level::WARN, "warning log");
        crate::dlog!(Level::DEBUG, "formatted {}", "log");
        crate::dlog!("message log", Level::DEBUG);
    });

    let bytes = output.lock().expect("lock log output").clone();
    let logs = String::from_utf8(bytes).expect("log output is UTF-8");
    for message in ["error log", "warning log", "formatted log", "message log"] {
        assert!(logs.contains(message));
    }
    assert!(logs.contains("source_file"));
    assert!(logs.contains("source_line"));
}

#[test]
fn disabled_logging_does_not_evaluate_arguments() {
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
        crate::tracing::Level::DEBUG
    );

    assert!(!formatted_argument_evaluated.get());
    assert!(!message_evaluated.get());
}

#[test]
fn production_filter_keeps_ws_warn_and_error_but_omits_normal_debug_events() {
    let output = Arc::new(Mutex::new(Vec::new()));
    let make_writer = {
        let output = Arc::clone(&output);
        move || SharedBuffer(Arc::clone(&output))
    };
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_max_level(Level::WARN)
        .with_writer(make_writer)
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        crate::dlog!(Level::DEBUG, "websocket disconnected normally");
        crate::dlog!(Level::DEBUG, "websocket idle timeout");
        crate::dlog!(Level::WARN, "websocket rate limit exceeded");
        crate::dlog!(Level::ERROR, "websocket persistence failed");
    });

    let bytes = output.lock().expect("lock log output").clone();
    let logs = String::from_utf8(bytes).expect("log output is UTF-8");
    assert!(!logs.contains("websocket disconnected normally"));
    assert!(!logs.contains("websocket idle timeout"));
    assert!(logs.contains("websocket rate limit exceeded"));
    assert!(logs.contains("websocket persistence failed"));
}
