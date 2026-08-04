use std::io;

use anyhow::Result;
use clap::{Args, CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

use crate::{
    commands::{
        get::{self, GetArgs},
        list::{self, ListArgs},
    },
    global_options::GlobalOptions,
};

#[derive(Debug, Args)]
struct GlobalArgs {
    /// Show detailed progress
    #[arg(short = 'v', long, global = true)]
    verbose: bool,
}

impl From<GlobalArgs> for GlobalOptions {
    fn from(args: GlobalArgs) -> Self {
        let GlobalArgs { verbose } = args;
        Self::new(verbose.into())
    }
}

#[derive(Debug, Parser)]
#[command(about, author, version)]
struct Cli {
    #[command(flatten)]
    global: GlobalArgs,

    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    fn run(self) -> Result<()> {
        let Self { global, command } = self;
        let global = GlobalOptions::from(global);
        command.run(&global)
    }
}

pub fn cli_main() -> Result<()> {
    Cli::parse().run()
}

#[derive(Debug, Subcommand)]
enum Commands {
    Completion(CompletionArgs),
    /// Clone a Git repository
    Get(GetArgs),
    /// List managed repositories
    List(ListArgs),
}

impl Commands {
    fn run(self, global: &GlobalOptions) -> Result<()> {
        match self {
            Self::Completion(args) => run_completion(&args, global),
            Self::Get(args) => get::run(&args, global),
            Self::List(args) => list::run(&args, global),
        }
    }
}

#[derive(Debug, Args)]
struct CompletionArgs {
    #[arg(value_enum, default_value_t = Shell::Bash)]
    shell: Shell,
}

fn run_completion(args: &CompletionArgs, _global: &GlobalOptions) -> Result<()> {
    let mut app = Cli::command();
    clap_complete::generate(args.shell, &mut app, "sm", &mut io::stdout());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Cli, Commands};
    use crate::{global_options::GlobalOptions, progress::ProgressDetail};
    use clap::{CommandFactory, Parser};

    fn parse(arguments: &[&str]) -> Cli {
        Cli::parse_from(["sm"].into_iter().chain(arguments.iter().copied()))
    }

    fn progress_detail(arguments: &[&str]) -> ProgressDetail {
        let cli = parse(arguments);
        GlobalOptions::from(cli.global).progress_detail()
    }

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn uses_normal_progress_by_default() {
        for arguments in [
            &["get", "owner/repository"][..],
            &["list"][..],
            &["completion"][..],
        ] {
            assert_eq!(progress_detail(arguments), ProgressDetail::Normal);
        }
    }

    #[test]
    fn accepts_global_verbose_before_and_after_the_subcommand() {
        for arguments in [
            &["--verbose", "get", "owner/repository"][..],
            &["get", "--verbose", "owner/repository"][..],
            &["get", "owner/repository", "--verbose"][..],
            &["-v", "get", "owner/repository"][..],
            &["get", "-v", "owner/repository"][..],
        ] {
            assert_eq!(progress_detail(arguments), ProgressDetail::Verbose);
        }
    }

    #[test]
    fn accepts_global_verbose_for_every_subcommand() {
        for arguments in [
            &["get", "owner/repository", "--verbose"][..],
            &["--verbose", "list"][..],
            &["list", "--verbose"][..],
            &["--verbose", "completion", "zsh"][..],
            &["completion", "zsh", "--verbose"][..],
        ] {
            assert_eq!(progress_detail(arguments), ProgressDetail::Verbose);
        }
    }

    #[test]
    fn option_terminator_keeps_verbose_as_a_get_argument() {
        let cli = parse(&["get", "--", "--verbose"]);
        let detail = GlobalOptions::from(cli.global).progress_detail();

        assert_eq!(detail, ProgressDetail::Normal);
        assert!(matches!(cli.command, Commands::Get(_)));
    }
}
