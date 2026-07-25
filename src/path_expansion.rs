use gix::config::{Path as ConfigPath, path::interpolate};
use std::path::{Path, PathBuf};
use thiserror::Error;

type HomeForUser = fn(&str) -> Option<PathBuf>;

#[derive(Debug)]
pub(crate) struct HomeDirectory(PathBuf);

impl HomeDirectory {
    pub(crate) fn discover() -> Result<Self, HomeDirectoryError> {
        gix::path::env::home_dir()
            .map(Self)
            .ok_or(HomeDirectoryError)
    }

    pub(crate) fn as_path(&self) -> &Path {
        &self.0
    }

    #[cfg(test)]
    pub(crate) fn from_path(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }
}

pub(crate) fn expand_git_path(
    input: ConfigPath<'_>,
    home: &HomeDirectory,
) -> Result<PathBuf, PathExpansionError> {
    expand_git_path_with_user_lookup(input, home, interpolate::home_for_user)
}

fn expand_git_path_with_user_lookup(
    input: ConfigPath<'_>,
    home: &HomeDirectory,
    home_for_user: HomeForUser,
) -> Result<PathBuf, PathExpansionError> {
    let path = input
        .interpolate(interpolate::Context {
            git_install_dir: None,
            home_dir: Some(home.as_path()),
            home_for_user: Some(home_for_user),
        })
        .map_err(PathExpansionError)?;

    if path == Path::new("~") {
        Ok(home.as_path().to_owned())
    } else {
        Ok(path.into_owned())
    }
}

#[derive(Debug, Error)]
#[error("could not determine the home directory")]
pub(crate) struct HomeDirectoryError;

#[derive(Debug, Error)]
#[error("could not expand path")]
pub(crate) struct PathExpansionError(#[source] interpolate::Error);

#[cfg(test)]
mod tests {
    use super::{
        HomeDirectory, PathExpansionError, expand_git_path, expand_git_path_with_user_lookup,
    };
    use gix::{
        bstr::BStr,
        config::{Path as ConfigPath, path::interpolate},
    };
    use std::{
        borrow::Cow,
        path::{Path, PathBuf},
    };

    fn home_directory() -> HomeDirectory {
        HomeDirectory::from_path("current-home")
    }

    fn config_path(input: &str) -> ConfigPath<'_> {
        ConfigPath::from(Cow::Borrowed(BStr::new(input.as_bytes())))
    }

    fn alice_home_directory(user: &str) -> Option<PathBuf> {
        assert_eq!(user, "alice");
        Some(PathBuf::from("alice-home"))
    }

    #[cfg(not(any(target_os = "windows", target_os = "android")))]
    fn missing_home_directory(user: &str) -> Option<PathBuf> {
        assert_eq!(user, "unknown");
        None
    }

    fn unexpected_named_user_lookup(user: &str) -> Option<PathBuf> {
        panic!("bare named user {user:?} must not trigger a lookup")
    }

    #[test]
    fn a_bare_tilde_means_the_current_home_directory() {
        let home = home_directory();

        let expanded = expand_git_path(config_path("~"), &home).unwrap();

        assert_eq!(expanded, home.as_path());
    }

    #[test]
    fn a_tilde_slash_prefix_is_relative_to_the_current_home_directory() {
        let home = home_directory();

        let expanded = expand_git_path(config_path("~/src/repository"), &home).unwrap();

        assert_eq!(expanded, home.as_path().join("src/repository"));
    }

    #[test]
    fn a_tilde_followed_only_by_slash_means_the_current_home_directory() {
        let home = home_directory();

        let expanded = expand_git_path(config_path("~/"), &home).unwrap();

        assert_eq!(expanded, home.as_path());
    }

    #[test]
    fn a_bare_named_user_is_left_unexpanded_to_match_gix() {
        let home = home_directory();

        let expanded = expand_git_path_with_user_lookup(
            config_path("~alice"),
            &home,
            unexpected_named_user_lookup,
        )
        .unwrap();

        assert_eq!(expanded, Path::new("~alice"));
    }

    #[cfg(not(any(target_os = "windows", target_os = "android")))]
    #[test]
    fn a_named_user_followed_by_slash_uses_the_named_users_home_directory() {
        let home = home_directory();

        let expanded = expand_git_path_with_user_lookup(
            config_path("~alice/src/repository"),
            &home,
            alice_home_directory,
        )
        .unwrap();

        assert_eq!(expanded, Path::new("alice-home/src/repository"));
    }

    #[cfg(not(any(target_os = "windows", target_os = "android")))]
    #[test]
    fn an_unknown_named_user_followed_by_slash_is_an_error() {
        let home = home_directory();

        let error = expand_git_path_with_user_lookup(
            config_path("~unknown/src/repository"),
            &home,
            missing_home_directory,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            PathExpansionError(interpolate::Error::Missing { .. })
        ));
    }

    #[cfg(any(target_os = "windows", target_os = "android"))]
    #[test]
    fn named_user_expansion_is_an_error_on_unsupported_platforms() {
        let home = home_directory();

        let error = expand_git_path_with_user_lookup(
            config_path("~alice/src/repository"),
            &home,
            alice_home_directory,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            PathExpansionError(interpolate::Error::UserInterpolationUnsupported)
        ));
    }

    #[test]
    fn only_a_leading_tilde_is_considered_for_expansion() {
        let home = home_directory();

        let expanded = expand_git_path(config_path("repositories/~/project"), &home).unwrap();

        assert_eq!(expanded, Path::new("repositories/~/project"));
    }

    #[test]
    fn an_ordinary_relative_path_is_left_unchanged() {
        let home = home_directory();

        let expanded = expand_git_path(config_path("src/repository"), &home).unwrap();

        assert_eq!(expanded, Path::new("src/repository"));
    }

    #[test]
    fn backslashes_do_not_trigger_current_or_named_user_expansion() {
        let home = home_directory();

        for input in [r"~\src\repository", r"~alice\src\repository"] {
            let expanded = expand_git_path(config_path(input), &home).unwrap();
            assert_eq!(expanded, Path::new(input));
        }
    }

    #[test]
    fn an_empty_git_path_is_an_interpolation_error() {
        let home = home_directory();

        let error = expand_git_path(config_path(""), &home).unwrap_err();

        assert!(matches!(
            error,
            PathExpansionError(interpolate::Error::Missing { .. })
        ));
    }

    #[test]
    fn a_git_install_prefix_requires_an_install_directory() {
        let home = home_directory();

        let error = expand_git_path(config_path("%(prefix)/share/srcmgr"), &home).unwrap_err();

        assert!(matches!(
            error,
            PathExpansionError(interpolate::Error::Missing { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_path_bytes_are_preserved_during_home_expansion() {
        use std::{ffi::OsStr, os::unix::ffi::OsStrExt};

        let home = home_directory();
        let input = ConfigPath::from(Cow::Borrowed(BStr::new(b"~/\xff")));

        let expanded = expand_git_path(input, &home).unwrap();

        assert_eq!(expanded, home.as_path().join(OsStr::from_bytes(b"\xff")));
    }
}
