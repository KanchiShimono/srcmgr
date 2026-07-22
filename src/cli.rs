use crate::commands::get;
use crate::commands::get::GetArgs;
use crate::commands::list;
use crate::commands::list::ListArgs;
use anyhow::Result;
use clap::{Args, CommandFactory, Parser, Subcommand};
use clap_complete::{Shell, generate};
use std::io;

#[derive(Debug, Parser)]
#[command(about, author, version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

pub fn cli_main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Completion(args) => generate_completion(&args),
        Commands::Get(args) => get::run(&args),
        Commands::List(args) => list::run(&args),
    }

    Ok(())
}

#[derive(Debug, Subcommand)]
enum Commands {
    Completion(CompletionArgs),
    /// Clone a Git repository
    Get(GetArgs),
    /// List managed repositories
    List(ListArgs),
}

#[derive(Debug, Args)]
struct CompletionArgs {
    #[arg(value_enum, default_value_t = Shell::Bash)]
    shell: Shell,
}

fn generate_completion(args: &CompletionArgs) {
    let mut app = Cli::command();
    generate(args.shell, &mut app, "sm", &mut io::stdout())
}
