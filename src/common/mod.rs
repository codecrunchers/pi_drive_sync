use slog::Drain;
use slog::*;

lazy_static::lazy_static! {
    ///Configure the global logger
    pub static ref LOG :Logger = {

        let debug = "debug".to_string();
        let info = "info".to_string();
        let warning = "warn".to_string();
        let error = "error".to_string();
        let trace = "trace".to_string();

        let log_level = match std::env::var("RUST_LOG") {
            Ok(l) =>  {
                if info == l {
                    slog::Level::Info
                }
                else if trace == l {
                    slog::Level::Trace
                }
                else if  debug  == l {
                    slog::Level::Debug
                }
                else if warning == l {
                    slog::Level::Warning
                }
                else if error == l {
                    slog::Level::Error
                }else {
                    slog::Level::Info
                }
            },
            Err(_) =>  slog::Level::Info

        };

        let decorator = slog_term::TermDecorator::new().build();
        let drain = slog_term::FullFormat::new(decorator).build();
        let drain = slog_async::Async::new( LevelFilter::new(drain, log_level).fuse() ).build().fuse(); //Lossy logger - chanel size dependent, will warn and drop

        slog::Logger::root(
            drain,
            o!(
                "version" => env!("CARGO_PKG_VERSION"),
                "service" => env!("CARGO_PKG_NAME")

            ))
    };

}
