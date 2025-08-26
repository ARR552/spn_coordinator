// logging.rs
use tracing_subscriber::{fmt, prelude::*};
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::fmt::time::OffsetTime;

pub fn init() {
     // Forward `log` -> `tracing` (ignore if already set)
    let _ = tracing_log::LogTracer::init();

    // Timer format (needs `time` features = ["macros"])
    let timer = OffsetTime::new(
        time::UtcOffset::UTC,
        time::macros::format_description!("[day]-[month]-[year] [hour]:[minute]:[second]"),
    );

    // Parse RUST_LOG or default to info
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // Build a non-panicking global subscriber
    let fmt_layer = fmt::layer()
        .with_timer(timer)
        .with_file(true)
        .with_line_number(true)
        .with_target(false)
        .compact();

    // This returns Err if someone already set a global subscriber — we ignore it.
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .try_init();
}
