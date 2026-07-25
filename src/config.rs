use crate::{
    canonical_dir::{CanonicalDir, CanonicalDirError},
    non_empty_vec::NonEmptyVec,
    path_expansion::{HomeDirectory, HomeDirectoryError, PathExpansionError, expand_git_path},
};
use gix::config::{File, Path as ConfigPath, Source};
use std::{collections::HashSet, str::Utf8Error};
use thiserror::Error;

#[derive(Debug)]
pub(crate) struct Config {
    roots: NonEmptyVec<CanonicalDir>,
    user_name: Option<String>,
}

#[derive(Debug, Error)]
pub(crate) enum ConfigError {
    #[error(transparent)]
    HomeDirectory(#[from] HomeDirectoryError),
    #[error(transparent)]
    Load(#[from] gix::config::file::init::from_paths::Error),
    #[error("could not interpolate srcmgr.root")]
    InterpolateRoot(#[source] PathExpansionError),
    #[error("srcmgr.root is not configured")]
    MissingRoot,
    #[error("invalid srcmgr.root")]
    InvalidRoot(#[source] CanonicalDirError),
    #[error("user.name is not valid UTF-8")]
    InvalidUserName(#[source] Utf8Error),
}

impl Config {
    pub(crate) fn load(home: &HomeDirectory) -> Result<Self, ConfigError> {
        let config = File::from_path_no_includes(home.as_path().join(".gitconfig"), Source::User)?;
        Self::from_file_with_home(&config, home)
    }

    fn from_file_with_home(config: &File<'_>, home: &HomeDirectory) -> Result<Self, ConfigError> {
        let root_paths = config
            .strings("srcmgr.root")
            .unwrap_or_default()
            .into_iter()
            .map(|root| {
                expand_git_path(ConfigPath::from(root), home).map_err(ConfigError::InterpolateRoot)
            })
            .collect::<Result<Vec<_>, ConfigError>>()?;
        let mut roots = root_paths
            .into_iter()
            .map(CanonicalDir::try_from)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(ConfigError::InvalidRoot)?;
        let mut seen = HashSet::new();
        roots.retain(|root| seen.insert(root.clone()));
        let roots = NonEmptyVec::try_from(roots).map_err(|_| ConfigError::MissingRoot)?;

        let user_name = config
            .string("user.name")
            .map(|name| {
                std::str::from_utf8(name.as_ref())
                    .map_err(ConfigError::InvalidUserName)
                    .map(str::to_owned)
            })
            .transpose()?;
        Ok(Self { roots, user_name })
    }

    #[cfg(test)]
    fn from_file(config: &File<'_>, home: &std::path::Path) -> Result<Self, ConfigError> {
        let home = HomeDirectory::from_path(home.to_owned());
        Self::from_file_with_home(config, &home)
    }

    pub(crate) fn roots(&self) -> &NonEmptyVec<CanonicalDir> {
        &self.roots
    }

    pub(crate) fn user_name(&self) -> Option<&str> {
        self.user_name.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, ConfigError};
    use crate::canonical_dir::CanonicalDirError;
    use gix::{bstr::ByteSlice, config::File};
    use std::{
        fs,
        path::{Path, PathBuf},
    };
    use tempfile::tempdir;

    fn root_paths(config: &Config) -> Vec<PathBuf> {
        config
            .roots()
            .iter()
            .map(|root| root.as_path().to_owned())
            .collect()
    }

    fn config_path(path: &Path) -> String {
        let escaped = path
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        format!("\"{escaped}\"")
    }

    #[test]
    fn reads_srcmgr_roots() {
        let temp = tempdir().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        fs::create_dir(&first).unwrap();
        fs::create_dir(&second).unwrap();
        let source = format!(
            "[srcmgr]\nroot = {}\nroot = {}\n",
            config_path(&first),
            config_path(&second)
        );
        let file = File::try_from(source.as_str()).unwrap();

        let config = Config::from_file(&file, temp.path()).unwrap();

        assert_eq!(
            root_paths(&config),
            [
                fs::canonicalize(first).unwrap(),
                fs::canonicalize(second).unwrap()
            ]
        );
    }

    #[test]
    fn rejects_missing_srcmgr_roots() {
        let file = File::try_from("[user]\nname = Test User\n").unwrap();

        let error = Config::from_file(&file, Path::new("/home/test")).unwrap_err();

        assert!(matches!(error, ConfigError::MissingRoot));
    }

    #[test]
    fn expands_home_directory_in_roots() {
        let temp = tempdir().unwrap();
        let home = temp.path();
        fs::create_dir_all(home.join("dev/src")).unwrap();
        fs::create_dir_all(home.join(".local/share/repositories")).unwrap();
        let file =
            File::try_from("[srcmgr]\nroot = ~/dev/src\nroot = ~/.local/share/repositories\n")
                .unwrap();

        let config = Config::from_file(&file, home).unwrap();

        assert_eq!(
            root_paths(&config),
            [
                fs::canonicalize(home.join("dev/src")).unwrap(),
                fs::canonicalize(home.join(".local/share/repositories")).unwrap(),
            ]
        );
    }

    #[test]
    fn expands_a_bare_tilde_to_the_home_directory() {
        let temp = tempdir().unwrap();
        let file = File::try_from("[srcmgr]\nroot = ~\n").unwrap();

        let config = Config::from_file(&file, temp.path()).unwrap();

        assert_eq!(
            root_paths(&config),
            [fs::canonicalize(temp.path()).unwrap()]
        );
    }

    #[test]
    fn rejects_empty_root_paths() {
        let temp = tempdir().unwrap();
        let file = File::try_from("[srcmgr]\nroot =\n").unwrap();

        let error = Config::from_file(&file, temp.path()).unwrap_err();

        assert!(matches!(error, ConfigError::InterpolateRoot(_)));
    }

    #[test]
    fn rejects_missing_root_paths() {
        let temp = tempdir().unwrap();
        let missing = temp.path().join("missing");
        let source = format!("[srcmgr]\nroot = {}\n", config_path(&missing));
        let file = File::try_from(source.as_str()).unwrap();

        let error = Config::from_file(&file, temp.path()).unwrap_err();

        assert!(matches!(
            &error,
            ConfigError::InvalidRoot(CanonicalDirError::Canonicalize { input, .. })
                if input == &missing
        ));
    }

    #[test]
    fn rejects_root_paths_that_are_not_directories() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("file");
        fs::write(&path, "not a directory").unwrap();
        let source = format!("[srcmgr]\nroot = {}\n", config_path(&path));
        let file = File::try_from(source.as_str()).unwrap();

        let error = Config::from_file(&file, temp.path()).unwrap_err();

        assert!(matches!(
            &error,
            ConfigError::InvalidRoot(CanonicalDirError::NotDirectory { input, .. })
                if input == &path
        ));
    }

    #[test]
    fn rejects_user_names_that_are_not_valid_utf8() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("root");
        fs::create_dir(&root).unwrap();
        let mut source =
            format!("[srcmgr]\nroot = {}\n[user]\nname = ", config_path(&root)).into_bytes();
        source.extend_from_slice(b"\xff\n");
        let file = File::try_from(source.as_bstr()).unwrap();

        let error = Config::from_file(&file, temp.path()).unwrap_err();

        assert!(matches!(error, ConfigError::InvalidUserName(_)));
    }

    #[test]
    fn rejects_all_roots_when_any_root_is_invalid() {
        let temp = tempdir().unwrap();
        let valid = temp.path().join("valid");
        let missing = temp.path().join("missing");
        fs::create_dir(&valid).unwrap();
        let source = format!(
            "[srcmgr]\nroot = {}\nroot = {}\n",
            config_path(&valid),
            config_path(&missing)
        );
        let file = File::try_from(source.as_str()).unwrap();

        assert!(Config::from_file(&file, temp.path()).is_err());
    }

    #[test]
    fn removes_duplicates_after_canonicalization_while_preserving_order() {
        let temp = tempdir().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        fs::create_dir_all(first.join("child")).unwrap();
        fs::create_dir(&second).unwrap();
        let first_alias = first.join("child").join("..");
        let source = format!(
            "[srcmgr]\nroot = {}\nroot = {}\nroot = {}\n",
            config_path(&first_alias),
            config_path(&second),
            config_path(&first)
        );
        let file = File::try_from(source.as_str()).unwrap();

        let config = Config::from_file(&file, temp.path()).unwrap();

        assert_eq!(
            root_paths(&config),
            [
                fs::canonicalize(first).unwrap(),
                fs::canonicalize(second).unwrap()
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn removes_symlink_duplicates_after_canonicalization() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let root = temp.path().join("root");
        let alias = temp.path().join("alias");
        fs::create_dir(&root).unwrap();
        symlink(&root, &alias).unwrap();
        let source = format!(
            "[srcmgr]\nroot = {}\nroot = {}\n",
            config_path(&alias),
            config_path(&root)
        );
        let file = File::try_from(source.as_str()).unwrap();

        let config = Config::from_file(&file, temp.path()).unwrap();

        assert_eq!(root_paths(&config), [fs::canonicalize(root).unwrap()]);
    }
}
