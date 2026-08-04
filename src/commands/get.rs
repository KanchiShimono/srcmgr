use crate::{
    clone_destination::{
        CloneDestination, DestinationParseError, ExplicitDestination, ManagedDestination,
    },
    config::{Config, ConfigError},
    destination_transaction::{CleanupError, DestinationTransaction, DestinationTransactionError},
    git_clone::{self, CloneError},
    global_options::GlobalOptions,
    path_expansion::HomeDirectory,
    progress::{ConsoleProgress, ProgressDetail},
    remote_repository::{RemoteRepository, RepositoryError},
};
use clap::Args;
use gix::Url;
use thiserror::Error;

#[derive(Debug, Args)]
pub(crate) struct GetArgs {
    /// Git repository to clone
    #[arg(allow_hyphen_values = true)]
    repository: String,

    /// Directory to clone into
    #[arg(allow_hyphen_values = true)]
    destination: Option<String>,
}

pub(crate) fn run(args: &GetArgs, global: &GlobalOptions) -> anyhow::Result<()> {
    // Configuration is deliberately loaded before any other validation, even
    // when an explicit destination was supplied.
    let home = HomeDirectory::discover()
        .map_err(ConfigError::from)
        .map_err(GetError::Config)?;
    let config = Config::load(&home).map_err(GetError::Config)?;
    let plan = ClonePlan::parse(args, &config, &home, global.progress_detail())?;
    run_clone_plan(plan)?;
    Ok(())
}

#[derive(Debug)]
struct ClonePlan {
    clone_url: Url,
    destination: CloneDestination,
    progress_detail: ProgressDetail,
}

impl ClonePlan {
    fn parse(
        args: &GetArgs,
        config: &Config,
        home: &HomeDirectory,
        progress_detail: ProgressDetail,
    ) -> Result<Self, GetError> {
        let repository = RemoteRepository::parse(&args.repository, config.user_name())?;
        let destination = match args.destination.as_deref() {
            Some(destination) => {
                CloneDestination::Explicit(ExplicitDestination::parse(destination, home)?)
            }
            None => {
                let root = config.roots().first().clone();
                CloneDestination::Managed(ManagedDestination::from_repository(root, &repository))
            }
        };

        Ok(Self {
            clone_url: repository.into_clone_url(),
            destination,
            progress_detail,
        })
    }
}

fn run_clone_plan(plan: ClonePlan) -> Result<(), GetError> {
    let ClonePlan {
        clone_url,
        destination,
        progress_detail,
    } = plan;
    let transaction = DestinationTransaction::begin(destination)?;
    let label = format!("Cloning into {}", transaction.path().display());
    let progress = ConsoleProgress::for_stderr(progress_detail);

    match progress.run(label, |progress| {
        git_clone::clone_repository(clone_url, transaction.path(), progress)
    }) {
        Ok(()) => {
            transaction.commit();
            Ok(())
        }
        Err(source) => match transaction.rollback() {
            Ok(()) => Err(GetError::Clone(source)),
            Err(cleanup) => Err(GetError::CloneAndCleanup { source, cleanup }),
        },
    }
}

#[derive(Debug, Error)]
enum GetError {
    #[error("failed to load configuration")]
    Config(#[source] ConfigError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error(transparent)]
    DestinationParse(#[from] DestinationParseError),
    #[error(transparent)]
    Destination(#[from] DestinationTransactionError),
    #[error(transparent)]
    Clone(#[from] CloneError),
    #[error("{source}; cleanup also failed: {cleanup}")]
    CloneAndCleanup {
        #[source]
        source: CloneError,
        cleanup: CleanupError,
    },
}

#[cfg(test)]
mod tests {
    use super::{ClonePlan, GetArgs};
    use crate::{
        clone_destination::CloneDestination, config::Config, path_expansion::HomeDirectory,
        progress::ProgressDetail,
    };
    use clap::Parser;
    use std::{
        fs,
        path::{Path, PathBuf},
    };
    use tempfile::tempdir;

    #[derive(Parser)]
    struct Options {
        #[command(flatten)]
        get: GetArgs,
    }

    fn parse(arguments: &[&str]) -> GetArgs {
        Options::parse_from(["get"].into_iter().chain(arguments.iter().copied())).get
    }

    fn git_config_value(value: &str) -> String {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    }

    fn load_config(home: &HomeDirectory, roots: &[PathBuf], user_name: Option<&str>) -> Config {
        let mut contents = String::from("[srcmgr]\n");
        for root in roots {
            contents.push_str(&format!(
                "root = {}\n",
                git_config_value(&root.to_string_lossy())
            ));
        }
        if let Some(user_name) = user_name {
            contents.push_str(&format!("[user]\nname = {}\n", git_config_value(user_name)));
        }

        fs::write(home.as_path().join(".gitconfig"), contents).unwrap();
        Config::load(home).unwrap()
    }

    #[test]
    fn requires_a_repository_and_accepts_an_optional_destination() {
        assert!(Options::try_parse_from(["get"]).is_err());

        let without_destination = parse(&["owner/repository"]);
        assert_eq!(without_destination.repository, "owner/repository");
        assert_eq!(without_destination.destination, None);

        let with_destination = parse(&["owner/repository", "checkout"]);
        assert_eq!(with_destination.repository, "owner/repository");
        assert_eq!(with_destination.destination.as_deref(), Some("checkout"));
    }

    #[test]
    fn accepts_hyphen_prefixed_positional_values() {
        let args = parse(&["--repository", "--destination"]);

        assert_eq!(args.repository, "--repository");
        assert_eq!(args.destination.as_deref(), Some("--destination"));
    }

    #[test]
    fn an_omitted_destination_uses_the_first_root_and_configured_user_name() {
        let temp = tempdir().unwrap();
        let home = HomeDirectory::from_path(temp.path());
        let first_root = temp.path().join("first-root");
        let second_root = temp.path().join("second-root");
        fs::create_dir(&first_root).unwrap();
        fs::create_dir(&second_root).unwrap();
        let config = load_config(
            &home,
            &[first_root.clone(), second_root],
            Some("example owner"),
        );
        let args = GetArgs {
            repository: "repository".to_owned(),
            destination: None,
        };

        let plan = ClonePlan::parse(&args, &config, &home, ProgressDetail::Normal).unwrap();

        assert_eq!(
            plan.clone_url.to_bstring().as_slice(),
            b"https://github.com/exampleowner/repository"
        );
        let expected_root = fs::canonicalize(first_root).unwrap();
        assert_eq!(
            plan.destination.path(),
            expected_root
                .join("github.com")
                .join("exampleowner")
                .join("repository")
        );
        let CloneDestination::Managed(destination) = &plan.destination else {
            panic!("an omitted destination must produce a managed destination");
        };
        assert_eq!(destination.managed_root().as_path(), expected_root);
    }

    #[test]
    fn an_explicit_destination_is_used_without_appending_remote_components() {
        let temp = tempdir().unwrap();
        let home = HomeDirectory::from_path(temp.path());
        let root = temp.path().join("root");
        fs::create_dir(&root).unwrap();
        let config = load_config(&home, &[root], None);
        let args = GetArgs {
            repository: "https://example.com/owner/repository".to_owned(),
            destination: Some("checkout".to_owned()),
        };

        let plan = ClonePlan::parse(&args, &config, &home, ProgressDetail::Verbose).unwrap();

        assert_eq!(
            plan.clone_url.to_bstring().as_slice(),
            b"https://example.com/owner/repository"
        );
        assert_eq!(plan.destination.path(), Path::new("checkout"));
        assert!(matches!(plan.destination, CloneDestination::Explicit(_)));
        assert_eq!(plan.progress_detail, ProgressDetail::Verbose);
    }
}
