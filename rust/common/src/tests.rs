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
fn logging_supports_every_level_and_macro_form() {
    let output = Arc::new(Mutex::new(Vec::new()));
    let make_writer = {
        let output = Arc::clone(&output);
        move || SharedBuffer(Arc::clone(&output))
    };
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_max_level(Level::TRACE)
        .with_writer(make_writer)
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        crate::dlog!(Level::ERROR, "error log");
        crate::dlog!(Level::WARN, "warning log");
        crate::dlog!(Level::INFO, "info log");
        crate::dlog!(Level::DEBUG, "formatted {}", "log");
        crate::dlog!("message log", Level::TRACE);
    });

    let bytes = output.lock().expect("lock log output").clone();
    let logs = String::from_utf8(bytes).expect("log output is UTF-8");
    for message in [
        "error log",
        "warning log",
        "info log",
        "formatted log",
        "message log",
    ] {
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
        crate::tracing::Level::TRACE
    );

    assert!(!formatted_argument_evaluated.get());
    assert!(!message_evaluated.get());
}
