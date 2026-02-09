//! Shipit CLI - build and serve projects anywhere

use clap::Parser;
use shipit::cli::{output::Output, Cli, Commands};
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    // Parse CLI arguments
    let cli = Cli::parse();

    // Initialize logging
    cli.init_logging();

    // Create output handler
    let output = Output::new(!cli.should_disable_colors());

    // Execute command
    let result = match cli.command {
        Commands::Auto(cmd) => cmd.execute(&output).await,
        Commands::Generate(cmd) => cmd.execute(&output),
        Commands::Plan(cmd) => cmd.execute(&output),
        Commands::Build(cmd) => cmd.execute(&output),
        Commands::Serve(cmd) => cmd.execute(&output).await,
        Commands::Deploy(cmd) => cmd.execute(&output).await,
        Commands::Config(cmd) => cmd.execute(&output),
    };

    // Handle errors
    match result {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            output.error(format!("Error: {}", e));
            if let Some(source) = e.source() {
                output.error(format!("   Caused by: {}", source));
            }
            log::debug!("Error backtrace: {:?}", e);
            ExitCode::FAILURE
        }
    }
}
