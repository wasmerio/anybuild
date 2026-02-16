//! Shipit CLI - build and serve projects anywhere

use clap::Parser;
use shipit::cli::{output::Output, Cli, Commands};
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    // Try to parse CLI arguments, handling routing to auto command
    let cli = parse_cli_with_auto_routing();

    // Initialize logging
    cli.init_logging();

    // Create output handler
    let output = Output::new(!cli.should_disable_colors());

    // Execute command (should always be Some after routing)
    let result = match cli.command {
        Some(Commands::Auto(cmd)) => cmd.execute(&output).await,
        Some(Commands::Generate(cmd)) => cmd.execute(&output),
        Some(Commands::Plan(cmd)) => cmd.execute(&output),
        Some(Commands::Build(cmd)) => cmd.execute(&output),
        Some(Commands::Serve(cmd)) => cmd.execute(&output).await,
        Some(Commands::Deploy(cmd)) => cmd.execute(&output).await,
        Some(Commands::Config(cmd)) => cmd.execute(&output),
        None => {
            // Should be unreachable after routing logic above
            unreachable!("Command should always be Some after parse_cli_with_auto_routing")
        }
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

/// Parse CLI arguments with custom routing to auto command
fn parse_cli_with_auto_routing() -> Cli {
    use clap::error::ErrorKind;

    // Try normal parsing first
    match Cli::try_parse() {
        Ok(cli) => {
            // Check if command is None - route to auto
            if cli.command.is_none() {
                // No subcommand provided, route to auto
                return route_to_auto_with_args();
            }
            cli
        }
        Err(e) => {
            // Check if error is due to unknown subcommand or unexpected argument
            if e.kind() == ErrorKind::InvalidSubcommand || e.kind() == ErrorKind::UnknownArgument {
                // Route to auto command
                route_to_auto_with_args()
            } else {
                // For other errors (like --help, --version), display them and exit
                e.exit();
            }
        }
    }
}

/// Route to auto command by reconstructing args
fn route_to_auto_with_args() -> Cli {
    use std::env;

    let args: Vec<String> = env::args().collect();

    // Insert "auto" after program name, let clap handle the rest
    // clap will handle global options regardless of position
    let mut new_args = vec![args[0].clone(), "auto".to_string()];
    new_args.extend_from_slice(&args[1..]);

    // Parse with auto command inserted
    Cli::parse_from(new_args)
}
