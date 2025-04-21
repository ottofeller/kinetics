/// Set up log levels, formatting, and other configurations for the logger
pub struct Logger;

impl Logger {
    pub fn init() -> Self {
        let mut builder = env_logger::Builder::new();

        // No logs shown by default, only human-friendlt messages
        // Enable logs output with "export RUST_LOG=error" in terminal
        builder.filter_level(log::LevelFilter::Off);

        builder.parse_default_env();
        builder.init();
        Logger
    }
}
