use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};
use thiserror::Error;

/// A path that referred to an existing directory when it was canonicalized.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct CanonicalDir(PathBuf);

impl CanonicalDir {
    pub(crate) fn as_path(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for CanonicalDir {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl TryFrom<PathBuf> for CanonicalDir {
    type Error = CanonicalDirError;

    fn try_from(input: PathBuf) -> Result<Self, Self::Error> {
        let canonical =
            fs::canonicalize(&input).map_err(|source| CanonicalDirError::Canonicalize {
                input: input.clone(),
                source,
            })?;
        let metadata = fs::metadata(&canonical).map_err(|source| CanonicalDirError::Metadata {
            input: input.clone(),
            canonical: canonical.clone(),
            source,
        })?;

        if metadata.is_dir() {
            Ok(Self(canonical))
        } else {
            Err(CanonicalDirError::NotDirectory { input, canonical })
        }
    }
}

impl From<CanonicalDir> for PathBuf {
    fn from(path: CanonicalDir) -> Self {
        path.0
    }
}

#[derive(Debug, Error)]
pub(crate) enum CanonicalDirError {
    #[error("could not canonicalize {}", .input.display())]
    Canonicalize {
        input: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(
        "could not inspect {} (canonicalized to {})",
        .input.display(),
        .canonical.display()
    )]
    Metadata {
        input: PathBuf,
        canonical: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{} is not a directory", CanonicalizedPath(.input, .canonical))]
    NotDirectory { input: PathBuf, canonical: PathBuf },
}

struct CanonicalizedPath<'a>(&'a Path, &'a Path);

impl fmt::Display for CanonicalizedPath<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == self.1 {
            write!(formatter, "{}", self.0.display())
        } else {
            write!(
                formatter,
                "{} (canonicalized to {})",
                self.0.display(),
                self.1.display()
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CanonicalDir, CanonicalDirError};
    use std::{fs, path::PathBuf};
    use tempfile::tempdir;

    #[test]
    fn canonicalizes_directories_and_converts_back_to_path_buf() {
        let temp = tempdir().unwrap();
        let directory = temp.path().join("directory");
        let child = directory.join("child");
        fs::create_dir_all(&child).unwrap();
        let alias = child.join("..");

        let canonical = CanonicalDir::try_from(alias).unwrap();
        let path: PathBuf = canonical.into();

        assert_eq!(path, fs::canonicalize(directory).unwrap());
    }

    #[test]
    fn rejects_missing_paths() {
        let path = tempdir().unwrap().path().join("missing");

        let error = CanonicalDir::try_from(path).unwrap_err();

        assert!(matches!(error, CanonicalDirError::Canonicalize { .. }));
    }

    #[test]
    fn rejects_non_directories() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("file");
        fs::write(&path, "not a directory").unwrap();

        let error = CanonicalDir::try_from(path).unwrap_err();

        assert!(matches!(error, CanonicalDirError::NotDirectory { .. }));
    }
}
