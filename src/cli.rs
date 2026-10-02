#[cfg(unix)]
use std::io::ErrorKind;
use std::io::Write;
use std::io::{self};

use anyhow::Context;
use anyhow::Result;
use clap::Args;
use clap::Command;
use clap::CommandFactory;
use clap::Parser;
use clap::Subcommand;
use clap_complete::Shell;
#[cfg(unix)]
use signal_hook::consts::SIGPIPE;
#[cfg(unix)]
use signal_hook::low_level;

use crate::commands::get::GetArgs;
use crate::commands::get::{self};
use crate::commands::list::ListArgs;
use crate::commands::list::{self};
use crate::global_options::GlobalOptions;

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
#[command(name = "sm", about, author, version)]
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

impl CompletionArgs {
    fn generate<W: Write>(&self, command: &mut Command, writer: &mut W) -> io::Result<()> {
        let mut buffer = Vec::new();
        clap_complete::generate(
            self.shell,
            command,
            command.get_name().to_string(),
            &mut buffer,
        );

        writer.write_all(&buffer)
    }
}

fn run_completion(args: &CompletionArgs, _global: &GlobalOptions) -> Result<()> {
    let mut command = Cli::command();
    execute_completion(args, &mut command, &mut io::stdout())
}

fn execute_completion<W: Write>(
    completion: &CompletionArgs,
    command: &mut Command,
    writer: &mut W,
) -> Result<()> {
    match completion.generate(command, writer) {
        Ok(()) => Ok(()),
        #[cfg(unix)]
        Err(error) if error.kind() == ErrorKind::BrokenPipe => terminate_with_sigpipe(),
        Err(error) => Err(error).context("failed to write shell completion script"),
    }
}

#[cfg(unix)]
fn terminate_with_sigpipe() -> Result<()> {
    low_level::emulate_default_handler(SIGPIPE)
        .context("failed to emulate the default SIGPIPE action")?;
    unreachable!("the default SIGPIPE action should terminate the process")
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::env;
    use std::io::Error;
    use std::io::ErrorKind;
    use std::io::Write;
    use std::io::{self};
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;
    #[cfg(unix)]
    use std::process::Command;

    use clap::CommandFactory;
    use clap::Parser;
    use clap_complete::Shell;
    #[cfg(unix)]
    use signal_hook::consts::SIGPIPE;

    use super::Cli;
    use super::Commands;
    use super::CompletionArgs;
    use crate::global_options::GlobalOptions;
    use crate::progress::ProgressDetail;

    struct FailingWriter(ErrorKind);

    impl Write for FailingWriter {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(Error::from(self.0))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

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

    #[test]
    fn completion_returns_regular_writer_error() {
        let error_kind = if cfg!(unix) {
            ErrorKind::StorageFull
        } else {
            ErrorKind::BrokenPipe
        };
        let completion = CompletionArgs { shell: Shell::Bash };
        let mut command = Cli::command();
        let mut writer = FailingWriter(error_kind);

        let error = super::execute_completion(&completion, &mut command, &mut writer).unwrap_err();

        assert_eq!(
            error.downcast_ref::<Error>().map(Error::kind),
            Some(error_kind)
        );
        assert!(
            error
                .to_string()
                .contains("failed to write shell completion script")
        );
    }

    #[cfg(unix)]
    #[test]
    fn completion_broken_pipe_terminates_with_sigpipe() {
        let output = Command::new(env::current_exe().unwrap())
            .args([
                "--exact",
                "cli::tests::completion_broken_pipe_subprocess",
                "--ignored",
            ])
            .env("SRCMGR_TEST_BROKEN_PIPE", "1")
            .output()
            .unwrap();

        assert_eq!(output.status.signal(), Some(SIGPIPE));
        assert!(
            output.stderr.is_empty(),
            "stderr should be empty: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "subprocess helper"]
    fn completion_broken_pipe_subprocess() {
        if env::var("SRCMGR_TEST_BROKEN_PIPE").as_deref() != Ok("1") {
            return;
        }

        let completion = CompletionArgs { shell: Shell::Bash };
        let mut command = Cli::command();
        let mut writer = FailingWriter(ErrorKind::BrokenPipe);

        super::execute_completion(&completion, &mut command, &mut writer).unwrap();
    }
}
