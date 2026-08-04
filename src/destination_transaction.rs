use crate::{clone_destination::CloneDestination, non_empty_vec::NonEmptyVec};
use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};
use thiserror::Error;

pub(crate) struct DestinationTransaction {
    destination: CloneDestination,
    created_parents: Vec<OwnedDirectory>,
    owned_destination: Option<OwnedDirectory>,
    active: bool,
}

impl DestinationTransaction {
    pub(crate) fn begin(
        destination: CloneDestination,
    ) -> Result<Self, DestinationTransactionError> {
        // This early check avoids creating parent directories for a destination
        // which is already known to exist.
        ensure_destination_absent(destination.path())?;

        let mut transaction = Self {
            destination,
            created_parents: Vec::new(),
            owned_destination: None,
            active: true,
        };
        if let Err(source) = transaction.create_parent_directories() {
            return Err(transaction.failure(source));
        }

        // Claim the destination atomically immediately after the required
        // second existence check. An empty directory created by this process is
        // accepted by gix and gives cleanup unambiguous ownership.
        if let Err(source) = ensure_destination_absent(transaction.destination.path()) {
            return Err(transaction.failure(source));
        }
        match fs::create_dir(transaction.destination.path()) {
            Ok(()) => {
                transaction.owned_destination = Some(OwnedDirectory::unverified(
                    transaction.destination.path().to_owned(),
                ));
                if let Err(source) = transaction
                    .owned_destination
                    .as_mut()
                    .expect("destination ownership was just recorded")
                    .verify()
                {
                    let path = transaction.destination.path().to_owned();
                    return Err(
                        transaction.failure(DestinationError::InspectCreated { path, source })
                    );
                }
            }
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
                let path = transaction.destination.path().to_owned();
                return Err(transaction.failure(DestinationError::Exists { path }));
            }
            Err(source) => {
                let path = transaction.destination.path().to_owned();
                return Err(transaction.failure(DestinationError::Create { path, source }));
            }
        }

        Ok(transaction)
    }

    pub(crate) fn path(&self) -> &Path {
        self.destination.path()
    }

    pub(crate) fn commit(mut self) {
        self.active = false;
    }

    pub(crate) fn rollback(mut self) -> Result<(), CleanupError> {
        self.active = false;
        self.cleanup()
    }

    fn create_parent_directories(&mut self) -> Result<(), DestinationError> {
        match &self.destination {
            CloneDestination::Managed(destination) => {
                ensure_directory(destination.managed_root().as_path(), true)?;
            }
            CloneDestination::Explicit(_) => {}
        }

        for parent in self.destination.parent_candidates() {
            match fs::create_dir(parent) {
                Ok(()) => {
                    self.created_parents
                        .push(OwnedDirectory::unverified(parent.clone()));
                    if let Err(source) = self
                        .created_parents
                        .last_mut()
                        .expect("created parent ownership was just recorded")
                        .verify()
                    {
                        return Err(DestinationError::InspectCreated {
                            path: parent.clone(),
                            source,
                        });
                    }
                }
                Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
                    ensure_directory(parent, false)?;
                }
                Err(source) => {
                    return Err(DestinationError::CreateParent {
                        path: parent.clone(),
                        source,
                    });
                }
            }
        }
        Ok(())
    }

    fn failure(self, source: DestinationError) -> DestinationTransactionError {
        match self.rollback() {
            Ok(()) => {
                DestinationTransactionError(DestinationTransactionErrorKind::Destination(source))
            }
            Err(cleanup) => DestinationTransactionError(
                DestinationTransactionErrorKind::DestinationAndCleanup { source, cleanup },
            ),
        }
    }

    fn cleanup(&mut self) -> Result<(), CleanupError> {
        let mut issues = Vec::new();

        if let Some(destination) = &self.owned_destination {
            remove_destination(destination, &mut issues);
        }

        for parent in self.created_parents.iter().rev() {
            match parent.current_state() {
                Ok(OwnedEntryState::Missing) => continue,
                Ok(OwnedEntryState::Replaced) => {
                    issues.push(CleanupIssue {
                        operation: "remove replaced parent directory",
                        path: parent.path.clone(),
                        source: io::Error::other(
                            "filesystem entry no longer matches the directory created by srcmgr",
                        ),
                    });
                    continue;
                }
                Ok(OwnedEntryState::Owned) => {}
                Err(source) => {
                    issues.push(CleanupIssue {
                        operation: "verify parent directory ownership",
                        path: parent.path.clone(),
                        source,
                    });
                    continue;
                }
            }

            match fs::remove_dir(&parent.path) {
                Ok(()) => {}
                Err(source) if source.kind() == io::ErrorKind::NotFound => {}
                Err(source) if source.kind() == io::ErrorKind::DirectoryNotEmpty => {}
                Err(source) => issues.push(CleanupIssue {
                    operation: "remove parent directory",
                    path: parent.path.clone(),
                    source,
                }),
            }
        }

        if issues.is_empty() {
            Ok(())
        } else {
            let issues = NonEmptyVec::try_from(issues)
                .expect("cleanup errors are only constructed from a non-empty list");
            Err(CleanupError { issues })
        }
    }
}

impl Drop for DestinationTransaction {
    fn drop(&mut self) {
        if self.active {
            let _ = self.cleanup();
        }
    }
}

fn ensure_directory(path: &Path, is_managed_root: bool) -> Result<(), DestinationError> {
    let metadata = if is_managed_root {
        fs::symlink_metadata(path)
    } else {
        fs::metadata(path)
    }
    .map_err(|source| DestinationError::InspectParent {
        path: path.to_owned(),
        source,
    })?;
    if metadata.is_dir() {
        Ok(())
    } else if is_managed_root {
        Err(DestinationError::ManagementRootNotDirectory {
            path: path.to_owned(),
        })
    } else {
        Err(DestinationError::ParentNotDirectory {
            path: path.to_owned(),
        })
    }
}

fn ensure_destination_absent(path: &Path) -> Result<(), DestinationError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(DestinationError::Exists {
            path: path.to_owned(),
        }),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(DestinationError::Inspect {
            path: path.to_owned(),
            source,
        }),
    }
}

#[derive(Debug)]
struct OwnedDirectory {
    path: PathBuf,
    identity: Option<DirectoryIdentity>,
}

impl OwnedDirectory {
    fn unverified(path: PathBuf) -> Self {
        Self {
            path,
            identity: None,
        }
    }

    fn verify(&mut self) -> io::Result<()> {
        let metadata = fs::symlink_metadata(&self.path)?;
        if !metadata.is_dir() {
            return Err(io::Error::other(
                "newly created filesystem entry is not a directory",
            ));
        }
        self.identity = Some(DirectoryIdentity::from_metadata(&self.path, &metadata)?);
        Ok(())
    }

    fn current_state(&self) -> io::Result<OwnedEntryState> {
        let Some(expected) = &self.identity else {
            return Err(io::Error::other(
                "filesystem identity was not recorded after directory creation",
            ));
        };
        let metadata = match fs::symlink_metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(source) if source.kind() == io::ErrorKind::NotFound => {
                return Ok(OwnedEntryState::Missing);
            }
            Err(source) => return Err(source),
        };
        if !metadata.is_dir() {
            return Ok(OwnedEntryState::Replaced);
        }

        let actual = DirectoryIdentity::from_metadata(&self.path, &metadata)?;
        Ok(if &actual == expected {
            OwnedEntryState::Owned
        } else {
            OwnedEntryState::Replaced
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OwnedEntryState {
    Missing,
    Owned,
    Replaced,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DirectoryIdentity {
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(windows)]
    volume: u32,
    #[cfg(windows)]
    file_index: u64,
    #[cfg(not(any(unix, windows)))]
    canonical_path: PathBuf,
}

impl DirectoryIdentity {
    fn from_metadata(_path: &Path, metadata: &fs::Metadata) -> io::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;

            Ok(Self {
                device: metadata.dev(),
                inode: metadata.ino(),
            })
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;

            let volume = metadata
                .volume_serial_number()
                .ok_or_else(|| io::Error::other("volume serial number is unavailable"))?;
            let file_index = metadata
                .file_index()
                .ok_or_else(|| io::Error::other("file index is unavailable"))?;
            Ok(Self { volume, file_index })
        }
        #[cfg(not(any(unix, windows)))]
        {
            Ok(Self {
                canonical_path: fs::canonicalize(_path)?,
            })
        }
    }
}

fn remove_destination(destination: &OwnedDirectory, issues: &mut Vec<CleanupIssue>) {
    match destination.current_state() {
        Ok(OwnedEntryState::Missing) => return,
        Ok(OwnedEntryState::Replaced) => {
            issues.push(CleanupIssue {
                operation: "remove replaced clone destination",
                path: destination.path.clone(),
                source: io::Error::other(
                    "filesystem entry no longer matches the directory created by srcmgr",
                ),
            });
            return;
        }
        Ok(OwnedEntryState::Owned) => {}
        Err(source) => {
            issues.push(CleanupIssue {
                operation: "verify clone destination ownership",
                path: destination.path.clone(),
                source,
            });
            return;
        }
    }

    if let Err(source) = fs::remove_dir_all(&destination.path) {
        issues.push(CleanupIssue {
            operation: "remove clone destination",
            path: destination.path.clone(),
            source,
        });
    }
}

#[derive(Debug, Error)]
#[error(transparent)]
pub(crate) struct DestinationTransactionError(DestinationTransactionErrorKind);

#[derive(Debug, Error)]
enum DestinationTransactionErrorKind {
    #[error(transparent)]
    Destination(DestinationError),
    #[error("{source}; cleanup also failed: {cleanup}")]
    DestinationAndCleanup {
        #[source]
        source: DestinationError,
        cleanup: CleanupError,
    },
}

impl From<DestinationError> for DestinationTransactionError {
    fn from(source: DestinationError) -> Self {
        Self(DestinationTransactionErrorKind::Destination(source))
    }
}

#[derive(Debug, Error)]
enum DestinationError {
    #[error("destination already exists: {}", .path.display())]
    Exists { path: PathBuf },
    #[error("could not inspect destination {}", .path.display())]
    Inspect {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not create destination {}", .path.display())]
    Create {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not verify newly created directory {}", .path.display())]
    InspectCreated {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not inspect parent directory {}", .path.display())]
    InspectParent {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not create parent directory {}", .path.display())]
    CreateParent {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("management root is not a directory: {}", .path.display())]
    ManagementRootNotDirectory { path: PathBuf },
    #[error("destination parent is not a directory: {}", .path.display())]
    ParentNotDirectory { path: PathBuf },
}

#[derive(Debug)]
struct CleanupIssue {
    operation: &'static str,
    path: PathBuf,
    source: io::Error,
}

#[derive(Debug)]
pub(crate) struct CleanupError {
    issues: NonEmptyVec<CleanupIssue>,
}

impl fmt::Display for CleanupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, issue) in self.issues.iter().enumerate() {
            if index > 0 {
                formatter.write_str("; ")?;
            }
            write!(
                formatter,
                "could not {} {}: {}",
                issue.operation,
                issue.path.display(),
                issue.source
            )?;
        }
        Ok(())
    }
}

impl Error for CleanupError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.issues.first().source)
    }
}

#[cfg(test)]
mod tests {
    use super::DestinationTransaction;
    use crate::{
        canonical_dir::CanonicalDir,
        clone_destination::{CloneDestination, ExplicitDestination, ManagedDestination},
        path_expansion::HomeDirectory,
        remote_repository::RemoteRepository,
    };
    use std::{fs, path::Path};

    fn explicit_destination(path: &Path) -> CloneDestination {
        let home = HomeDirectory::from_path("unused-home");
        let input = path
            .to_str()
            .expect("temporary test paths must be valid UTF-8");
        CloneDestination::Explicit(ExplicitDestination::parse(input, &home).unwrap())
    }

    #[test]
    fn commit_keeps_the_destination_and_checkout_contents() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("new").join("namespace").join("repository");
        let transaction =
            DestinationTransaction::begin(explicit_destination(&destination)).unwrap();
        fs::write(transaction.path().join("README.md"), "checkout contents").unwrap();

        transaction.commit();

        assert_eq!(
            fs::read_to_string(destination.join("README.md")).unwrap(),
            "checkout contents"
        );
    }

    #[test]
    fn rollback_removes_the_clone_tree_and_new_empty_parents_only() {
        let temp = tempfile::tempdir().unwrap();
        let existing_parent = temp.path().join("existing");
        let new_parent = existing_parent.join("new");
        let destination = new_parent.join("namespace").join("repository");
        fs::create_dir(&existing_parent).unwrap();
        let transaction =
            DestinationTransaction::begin(explicit_destination(&destination)).unwrap();
        fs::write(transaction.path().join("partial-clone"), "contents").unwrap();

        transaction.rollback().unwrap();

        assert!(existing_parent.is_dir());
        assert!(!new_parent.exists());
        assert!(!destination.exists());
    }

    #[test]
    fn rollback_keeps_a_created_parent_that_has_since_become_non_empty() {
        let temp = tempfile::tempdir().unwrap();
        let created_parent = temp.path().join("created");
        let destination = created_parent.join("repository");
        let transaction =
            DestinationTransaction::begin(explicit_destination(&destination)).unwrap();
        fs::write(created_parent.join("independent-file"), "keep").unwrap();

        transaction.rollback().unwrap();

        assert!(!destination.exists());
        assert_eq!(
            fs::read_to_string(created_parent.join("independent-file")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn rollback_of_a_managed_destination_never_removes_the_managed_root() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("managed-root");
        fs::create_dir(&root).unwrap();
        let managed_root = CanonicalDir::try_from(root.clone()).unwrap();
        let repository =
            RemoteRepository::parse("https://example.com/owner/repository.git", None).unwrap();
        let destination = CloneDestination::Managed(ManagedDestination::from_repository(
            managed_root,
            &repository,
        ));
        let transaction = DestinationTransaction::begin(destination).unwrap();
        fs::write(transaction.path().join("partial-clone"), "contents").unwrap();

        transaction.rollback().unwrap();

        assert!(root.is_dir());
        assert!(!root.join("example.com").exists());
    }

    #[test]
    fn rollback_refuses_to_delete_a_replacement_at_the_destination_path() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("repository");
        let transaction =
            DestinationTransaction::begin(explicit_destination(&destination)).unwrap();
        fs::remove_dir(&destination).unwrap();
        fs::write(&destination, "replacement").unwrap();

        let result = transaction.rollback();

        assert!(result.is_err());
        assert_eq!(fs::read_to_string(destination).unwrap(), "replacement");
    }

    #[test]
    fn begin_does_not_recreate_a_managed_root_removed_after_configuration() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("managed-root");
        fs::create_dir(&root).unwrap();
        let managed_root = CanonicalDir::try_from(root.clone()).unwrap();
        fs::remove_dir(&root).unwrap();
        let repository =
            RemoteRepository::parse("https://example.com/owner/repository.git", None).unwrap();
        let destination = CloneDestination::Managed(ManagedDestination::from_repository(
            managed_root,
            &repository,
        ));

        let result = DestinationTransaction::begin(destination);

        assert!(result.is_err());
        assert!(!root.exists());
    }

    #[test]
    fn an_existing_destination_is_rejected_without_modifying_it() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("repository");
        fs::write(&destination, "existing contents").unwrap();

        let result = DestinationTransaction::begin(explicit_destination(&destination));

        assert!(result.is_err());
        assert_eq!(
            fs::read_to_string(destination).unwrap(),
            "existing contents"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_destination_symlink_is_still_considered_existing() {
        use std::os::unix::fs as unix_fs;

        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("repository");
        unix_fs::symlink(temp.path().join("missing-target"), &destination).unwrap();

        let result = DestinationTransaction::begin(explicit_destination(&destination));

        assert!(result.is_err());
        assert!(
            fs::symlink_metadata(destination)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
}
