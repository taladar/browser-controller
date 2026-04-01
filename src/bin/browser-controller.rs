#![doc = include_str!("../../README.md")]

use tracing::{Instrument as _, instrument};
use tracing_subscriber::{
    EnvFilter, Layer as _, Registry, filter::LevelFilter, layer::SubscriberExt as _,
    util::SubscriberInitExt as _,
};

/// Error enum for the application
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// error reading environment variable
    #[error("error when retrieving environment variable: {0}")]
    EnvVarError(
        #[source]
        #[from]
        std::env::VarError,
    ),
    /// error in clap
    #[error("error in CLI option parsing: {0}")]
    ClapError(
        #[source]
        #[from]
        clap::Error,
    ),
    /// error parsing log filter
    #[error("error parsing log filter: {0}")]
    LogFilterParseError(
        #[source]
        #[from]
        tracing_subscriber::filter::ParseError,
    ),
    /// error joining task
    #[error("error joining task: {0}")]
    JoinError(
        #[source]
        #[from]
        tokio::task::JoinError,
    ),
    /// error constructing tracing-journald layer
    #[cfg(target_os = "linux")]
    #[error("error constructing tracing-journald layer: {0}")]
    TracingJournaldError(#[source] std::io::Error),
    /// error generating man pages
    #[error("error generating man pages: {0}")]
    GenerateManpageError(#[source] std::io::Error),
    /// error generating shell completion
    #[error("error generating shell completion: {0}")]
    GenerateShellCompletionError(#[source] std::io::Error),
}

/// parse a `time::OffsetDateTime` as a clap parameter
///
/// # Errors
///
/// fails if parsing the OffsetDateTime fails
fn parse_offset_date_time(s: &str) -> Result<time::OffsetDateTime, time::error::Parse> {
    time::OffsetDateTime::parse(
        s,
        time::macros::format_description!(
            "[year]-[month]-[day] [hour]:[minute]:[second] [offset_hour sign:mandatory]:[offset_minute]:[offset_second]"
        ),
    )
}

/// Parameters for foo subcommand
#[derive(clap::Parser, Debug, Clone)]
pub struct FooParameters {
    /// filename parameter for foo
    #[clap(long)]
    pub input_file: std::path::PathBuf,
}

/// Parameters for bar subcommand
#[derive(clap::Parser, Debug, Clone)]
pub struct BarParameters {
    /// time parameter for bar
    #[clap(long, help_heading = "Bar time", value_name = "YYYY-MM-DD HH:MM:SS +00:00:00", value_parser = parse_offset_date_time)]
    bar_time: time::OffsetDateTime,
    /// duration parameter for bar
    #[clap(long, help_heading = "Bar duration", default_value = "1s")]
    bar_duration: humantime::Duration,
}

/// which subcommand to call
#[derive(clap::Parser, Debug)]
pub enum Command {
    /// Call foo subcommand
    Foo(FooParameters),
    /// Call bar subcommand
    Bar(BarParameters),
    /// Generate man page
    GenerateManpage {
        /// target dir for man page generation
        #[clap(long)]
        output_dir: std::path::PathBuf,
    },
    /// Generate shell completion
    GenerateShellCompletion {
        /// output file for shell completion generation
        #[clap(long)]
        output_file: std::path::PathBuf,
        /// which shell
        #[clap(long)]
        shell: clap_complete::aot::Shell,
    },
}

/// The Clap type for all the commandline parameters
#[derive(clap::Parser, Debug)]
#[clap(name = "browser-controller",
       about = clap::crate_description!(),
       author = clap::crate_authors!(),
       version = clap::crate_version!(),
       )]
struct Options {
    /// which subcommand to use
    #[clap(subcommand)]
    command: Command,
}

/// implementation of the foo subcommand
///
/// # Errors
///
/// fails if the implementation of foo fails
#[instrument]
async fn foo_command(foo_parameters: FooParameters) -> Result<(), crate::Error> {
    // implementation of foo subcommand

    let foo_subtask_a_span = tracing::info_span!(
        "perform subtask a",
        subtask_a_intermediate_result = tracing::field::Empty
    );
    let handle = tokio::task::spawn(
        async move {
            tracing::info!("Starting foo subtask a");
            tracing::Span::current().record("subtask_a_intermediate_result", 27);
            tracing::info!(field_on_event = 38, "Continuing foo subtask a");
        }
        .instrument(foo_subtask_a_span),
    );

    handle.await?;

    Ok(())
}

/// implementation of the bar subcommand
///
/// # Errors
///
/// fails if the implementation of bar fails
#[instrument]
async fn bar_command(bar_parameters: BarParameters) -> Result<(), crate::Error> {
    // implementation of bar subcommand
    Ok(())
}

/// The main behaviour of the binary should go here
///
/// # Errors
///
/// fails if the main behavior of the application fails
#[instrument]
async fn do_stuff() -> Result<(), crate::Error> {
    let options = <Options as clap::Parser>::parse();
    tracing::debug!("{:#?}", options);

    // main code either goes here or into the individual subcommands

    match options.command {
        Command::Foo(foo_parameters) => {
            // might need extra parameters from options shared by subcommands
            foo_command(foo_parameters).await?;
        }
        Command::Bar(bar_parameters) => {
            // might need extra parameters from options shared by subcommands
            bar_command(bar_parameters).await?;
        }
        Command::GenerateManpage { output_dir } => {
            // generate man pages
            clap_mangen::generate_to(<Options as clap::CommandFactory>::command(), output_dir)
                .map_err(crate::Error::GenerateManpageError)?;
        }
        Command::GenerateShellCompletion { output_file, shell } => {
            let mut f = std::fs::File::create(output_file)
                .map_err(crate::Error::GenerateShellCompletionError)?;
            let mut c = <Options as clap::CommandFactory>::command();
            clap_complete::generate(shell, &mut c, "browser-controller", &mut f);
        }
    }

    Ok(())
}

/// The main function mainly just handles setting up tracing
/// and handling any Err Results.
#[tokio::main]
async fn main() -> Result<(), Error> {
    let terminal_env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::WARN.into())
        .parse(std::env::var("RUST_LOG").unwrap_or_else(|_| "warn".to_string()))?;
    let file_env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::TRACE.into())
        .parse(std::env::var("BROWSER_CONTROLLER_LOG").unwrap_or_else(|_| "trace".to_string()))?;
    #[cfg(target_os = "linux")]
    let journald_env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::TRACE.into())
        .parse(
            std::env::var("BROWSER_CONTROLLER_JOURNALD_LOG").unwrap_or_else(|_| "info".to_string()),
        )?;
    let registry = Registry::default();
    let registry =
        registry.with(tracing_subscriber::fmt::Layer::default().with_filter(terminal_env_filter));
    let log_dir = std::env::var("BROWSER_CONTROLLER_LOG_DIR");
    let file_layer = if let Ok(log_dir) = log_dir {
        let log_file = if let Ok(log_file) = std::env::var("BROWSER_CONTROLLER_LOG_FILE") {
            log_file
        } else {
            "browser_controller.log".to_string()
        };
        let file_appender = tracing_appender::rolling::never(log_dir, log_file);
        Some(
            tracing_subscriber::fmt::Layer::default()
                .with_writer(file_appender)
                .with_filter(file_env_filter),
        )
    } else {
        None
    };
    let registry = registry.with(file_layer);
    #[cfg(target_os = "linux")]
    let registry = registry.with(
        tracing_journald::layer()
            .map_err(crate::Error::TracingJournaldError)?
            .with_filter(journald_env_filter),
    );
    registry.init();
    log_panics::init();
    #[expect(
        clippy::print_stderr,
        reason = "This is the final print in our error chain and we already log this with tracing above but depending on log level the tracing output is not seen by the user"
    )]
    match do_stuff().await {
        Ok(()) => (),
        Err(e) => {
            tracing::error!("{e}");
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
    tracing::debug!("Exiting");
    Ok(())
}

#[cfg(test)]
mod test {
    //use super::*;
    //use pretty_assertions::{assert_eq, assert_ne};
}
