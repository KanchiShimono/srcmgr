use crate::{
    canonical_dir::CanonicalDir,
    path_expansion::{self, HomeDirectory, PathExpansionError},
    remote_repository::RemoteRepository,
};
use gix::bstr::BStr;
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug)]
pub(crate) enum CloneDestination {
    Managed(ManagedDestination),
    Explicit(ExplicitDestination),
}

impl CloneDestination {
    pub(crate) fn path(&self) -> &Path {
        match self {
            Self::Managed(destination) => &destination.path,
            Self::Explicit(destination) => &destination.path,
        }
    }

    pub(crate) fn parent_candidates(&self) -> &[PathBuf] {
        match self {
            Self::Managed(destination) => &destination.parent_candidates,
            Self::Explicit(destination) => &destination.parent_candidates,
        }
    }
}

#[derive(Debug)]
pub(crate) struct ManagedDestination {
    path: PathBuf,
    parent_candidates: Vec<PathBuf>,
    managed_root: CanonicalDir,
}

impl ManagedDestination {
    pub(crate) fn from_repository(
        managed_root: CanonicalDir,
        repository: &RemoteRepository,
    ) -> Self {
        let mut path = managed_root.as_path().to_owned();
        let mut parent_candidates = Vec::new();
        let mut components = repository.managed_path_components().peekable();
        while let Some(component) = components.next() {
            path.push(component);
            if components.peek().is_some() {
                parent_candidates.push(path.clone());
            }
        }

        Self {
            path,
            parent_candidates,
            managed_root,
        }
    }

    pub(crate) fn managed_root(&self) -> &CanonicalDir {
        &self.managed_root
    }
}

#[derive(Debug)]
pub(crate) struct ExplicitDestination {
    path: PathBuf,
    parent_candidates: Vec<PathBuf>,
}

impl ExplicitDestination {
    pub(crate) fn parse(input: &str, home: &HomeDirectory) -> Result<Self, DestinationParseError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(DestinationParseError::Empty);
        }

        let input_path = Path::new(input);
        let path = if input.starts_with('~') {
            let input = gix::config::Path::from(Cow::Borrowed(BStr::new(input.as_bytes())));
            path_expansion::expand_git_path(input, home)
                .map_err(DestinationParseError::Interpolate)?
        } else {
            input_path.to_owned()
        };
        let parent_candidates = explicit_parent_candidates(&path);

        Ok(Self {
            path,
            parent_candidates,
        })
    }
}

fn explicit_parent_candidates(path: &Path) -> Vec<PathBuf> {
    let Some(parent) = path.parent() else {
        return Vec::new();
    };
    let mut candidates = parent
        .ancestors()
        .take_while(|ancestor| !ancestor.as_os_str().is_empty())
        .map(Path::to_owned)
        .collect::<Vec<_>>();
    candidates.reverse();
    candidates
}

#[derive(Debug, Error)]
pub(crate) enum DestinationParseError {
    #[error("destination must not be empty")]
    Empty,
    #[error("could not expand destination")]
    Interpolate(#[source] PathExpansionError),
}

#[cfg(test)]
mod tests {
    use super::{CloneDestination, DestinationParseError, ExplicitDestination, ManagedDestination};
    use crate::{
        canonical_dir::CanonicalDir, path_expansion::HomeDirectory,
        remote_repository::RemoteRepository,
    };
    use std::path::{Path, PathBuf};

    #[test]
    fn managed_destination_preserves_its_root_and_uses_repository_components() {
        let temp = tempfile::tempdir().unwrap();
        let managed_root = CanonicalDir::try_from(temp.path().to_owned()).unwrap();
        let repository =
            RemoteRepository::parse("https://example.com/owner/repository.git", None).unwrap();
        let managed = ManagedDestination::from_repository(managed_root.clone(), &repository);
        assert_eq!(managed.managed_root(), &managed_root);

        let destination = CloneDestination::Managed(managed);
        assert_eq!(
            destination.path(),
            managed_root
                .as_path()
                .join("example.com")
                .join("owner")
                .join("repository")
        );
        assert_eq!(
            destination.parent_candidates(),
            [
                managed_root.as_path().join("example.com"),
                managed_root.as_path().join("example.com").join("owner"),
            ]
        );
    }

    #[test]
    fn explicit_destination_trims_but_does_not_interpolate_a_non_tilde_path() {
        let home = HomeDirectory::from_path("current-home");

        let destination = CloneDestination::Explicit(
            ExplicitDestination::parse("  %(prefix)/owner/repository  ", &home).unwrap(),
        );

        assert_eq!(
            destination.path(),
            Path::new("%(prefix)").join("owner").join("repository")
        );
        assert_eq!(
            destination.parent_candidates(),
            [
                PathBuf::from("%(prefix)"),
                PathBuf::from("%(prefix)").join("owner"),
            ]
        );
    }

    #[test]
    fn explicit_destination_expands_a_current_home_prefix_before_planning_parents() {
        let home = HomeDirectory::from_path("current-home");

        let destination = CloneDestination::Explicit(
            ExplicitDestination::parse("~/workspace/repository", &home).unwrap(),
        );

        assert_eq!(
            destination.path(),
            home.as_path().join("workspace").join("repository")
        );
        assert_eq!(
            destination.parent_candidates(),
            [
                PathBuf::from("current-home"),
                PathBuf::from("current-home").join("workspace"),
            ]
        );
    }

    #[test]
    fn explicit_bare_named_user_is_left_unexpanded_to_match_gix() {
        let home = HomeDirectory::from_path("current-home");

        let destination =
            CloneDestination::Explicit(ExplicitDestination::parse("~alice", &home).unwrap());

        assert_eq!(destination.path(), Path::new("~alice"));
        assert!(destination.parent_candidates().is_empty());
    }

    #[test]
    fn explicit_destination_rejects_blank_input() {
        let home = HomeDirectory::from_path("current-home");

        let error = ExplicitDestination::parse(" \t\n ", &home).unwrap_err();

        assert!(matches!(error, DestinationParseError::Empty));
    }
}
