use crate::non_empty_vec::NonEmptyVec;
use gix::bstr::BStr;
use std::path::{Component, PathBuf};
use thiserror::Error;

#[derive(Debug)]
pub(crate) struct RemoteRepository {
    clone_url: gix::Url,
    host: SafePathComponent,
    path: NonEmptyVec<SafePathComponent>,
}

impl RemoteRepository {
    pub(crate) fn parse(input: &str, user_name: Option<&str>) -> Result<Self, RepositoryError> {
        if input
            .get(.."file:".len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("file:"))
        {
            return Err(RepositoryError::LocalRepository);
        }

        let clone_url = match ShorthandRepository::parse(input)? {
            Some(ShorthandRepository::Owned { owner, repository }) => {
                github_url(&owner, &repository)?
            }
            Some(ShorthandRepository::CurrentUser { repository }) => {
                let owner = GithubOwner::from_user_name(user_name)?;
                github_url(&owner, &repository)?
            }
            None => {
                if remote_path_contains_encoded_separator(input) {
                    return Err(RepositoryError::EncodedPathSeparator);
                }
                gix::Url::try_from(input)
                    .map_err(Box::new)
                    .map_err(RepositoryError::ParseUrl)?
            }
        };

        if !matches!(
            clone_url.scheme,
            gix::url::Scheme::Http
                | gix::url::Scheme::Https
                | gix::url::Scheme::Ssh
                | gix::url::Scheme::Git
        ) {
            return Err(match clone_url.scheme {
                gix::url::Scheme::File => RepositoryError::LocalRepository,
                ref scheme => RepositoryError::UnsupportedScheme(scheme.to_string()),
            });
        }

        let host = clone_url
            .host()
            .ok_or(RepositoryError::MissingHost)
            .and_then(|host| {
                SafePathComponent::new(host.as_bytes()).map_err(RepositoryError::InvalidHost)
            })?;
        let path = destination_path(&clone_url.path)?;

        Ok(Self {
            clone_url,
            host,
            path,
        })
    }

    pub(crate) fn managed_path_components(&self) -> impl Iterator<Item = PathBuf> + '_ {
        std::iter::once(&self.host)
            .chain(self.path.iter())
            .map(SafePathComponent::to_path_buf)
    }

    pub(crate) fn into_clone_url(self) -> gix::Url {
        self.clone_url
    }
}

fn remote_path_contains_encoded_separator(input: &str) -> bool {
    let path = if let Some(scheme_end) = input.find("://") {
        let authority_and_path = &input[scheme_end + "://".len()..];
        authority_and_path
            .find('/')
            .map(|path_start| &authority_and_path[path_start..])
            .unwrap_or_default()
    } else {
        let colon = if input.starts_with('[') {
            input.find(']').and_then(|bracket_end| {
                input[bracket_end + 1..]
                    .find(':')
                    .map(|colon| bracket_end + 1 + colon)
            })
        } else {
            input.find(':')
        };
        colon
            .and_then(|colon| input.get(colon + 1..))
            .unwrap_or_default()
    };

    path.as_bytes().windows(3).any(|window| {
        window[0] == b'%'
            && ((window[1] == b'2' && window[2].eq_ignore_ascii_case(&b'f'))
                || (window[1] == b'5' && window[2].eq_ignore_ascii_case(&b'c')))
    })
}

fn github_url(
    owner: &GithubOwner,
    repository: &RepositoryName,
) -> Result<gix::Url, RepositoryError> {
    gix::Url::try_from(format!(
        "https://github.com/{}/{}",
        owner.as_str(),
        repository.as_str()
    ))
    .map_err(Box::new)
    .map_err(RepositoryError::ParseUrl)
}

#[derive(Debug)]
enum ShorthandRepository {
    Owned {
        owner: GithubOwner,
        repository: RepositoryName,
    },
    CurrentUser {
        repository: RepositoryName,
    },
}

impl ShorthandRepository {
    fn parse(input: &str) -> Result<Option<Self>, RepositoryError> {
        if input.contains("://") || input.contains(':') {
            return Ok(None);
        }

        let mut components = input.split('/');
        let first = components.next().expect("split always yields one item");
        let second = components.next();
        let third = components.next();

        match (second, third) {
            (None, None) => Ok(Some(Self::CurrentUser {
                repository: RepositoryName::parse(first)?,
            })),
            (Some(repository), None) => Ok(Some(Self::Owned {
                owner: GithubOwner::parse(first)?,
                repository: RepositoryName::parse(repository)?,
            })),
            _ => Ok(None),
        }
    }
}

#[derive(Debug)]
struct GithubOwner(String);

impl GithubOwner {
    fn parse(input: &str) -> Result<Self, RepositoryError> {
        if is_github_owner(input) {
            Ok(Self(input.to_owned()))
        } else {
            Err(RepositoryError::InvalidOwner)
        }
    }

    fn from_user_name(user_name: Option<&str>) -> Result<Self, RepositoryError> {
        let user_name = user_name.ok_or(RepositoryError::MissingUserName)?;
        let owner = user_name
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>();
        Self::parse(&owner).map_err(|_| RepositoryError::InvalidUserName)
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_github_owner(input: &str) -> bool {
    !input.is_empty()
        && input
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

#[derive(Debug)]
struct RepositoryName(String);

impl RepositoryName {
    fn parse(input: &str) -> Result<Self, RepositoryError> {
        let valid = !input.is_empty()
            && input != "."
            && input != ".."
            && input
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'));
        if valid {
            Ok(Self(input.to_owned()))
        } else {
            Err(RepositoryError::InvalidRepositoryName)
        }
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug)]
struct SafePathComponent(Vec<u8>);

impl SafePathComponent {
    fn new(value: &[u8]) -> Result<Self, PathComponentError> {
        if value.is_empty() {
            return Err(PathComponentError::Empty);
        }
        if value == b"." {
            return Err(PathComponentError::CurrentDirectory);
        }
        if value == b".." {
            return Err(PathComponentError::ParentDirectory);
        }
        if value
            .iter()
            .any(|byte| matches!(byte, b'/' | b'\\' | b'\0'))
        {
            return Err(PathComponentError::Separator);
        }

        let path = gix::path::from_bstr(BStr::new(value));
        let mut components = path.components();
        if !matches!(
            (components.next(), components.next()),
            (Some(Component::Normal(_)), None)
        ) {
            return Err(PathComponentError::PlatformPathSyntax);
        }
        Ok(Self(value.to_owned()))
    }

    fn to_path_buf(&self) -> PathBuf {
        let mut path = PathBuf::new();
        path.push(gix::path::from_bstr(BStr::new(&self.0)).as_ref());
        path
    }
}

fn destination_path(
    remote_path: &gix::bstr::BString,
) -> Result<NonEmptyVec<SafePathComponent>, RepositoryError> {
    let remote_path: &[u8] = remote_path.as_ref();
    let path = remote_path.strip_prefix(b"/").unwrap_or(remote_path);
    if path.is_empty() {
        return Err(RepositoryError::MissingRepositoryPath);
    }
    let mut raw_components = path.split(|byte| *byte == b'/').collect::<Vec<_>>();

    if let Some(repository) = raw_components.last_mut()
        && let Some(without_suffix) = repository.strip_suffix(b".git")
    {
        *repository = without_suffix;
    }

    let components = raw_components
        .into_iter()
        .map(|component| {
            SafePathComponent::new(component).map_err(RepositoryError::InvalidRemotePath)
        })
        .collect::<Result<Vec<_>, _>>()?;
    NonEmptyVec::try_from(components).map_err(|_| RepositoryError::MissingRepositoryPath)
}

#[derive(Debug, Error)]
pub(crate) enum RepositoryError {
    #[error("local repositories and file URLs are not supported")]
    LocalRepository,
    #[error("repository URL could not be parsed")]
    ParseUrl(#[source] Box<gix::url::parse::Error>),
    #[error("repository URL scheme {0:?} is not supported")]
    UnsupportedScheme(String),
    #[error("repository URL does not contain a host")]
    MissingHost,
    #[error("repository URL contains an invalid host name")]
    InvalidHost(#[source] PathComponentError),
    #[error("repository URL does not contain a repository path")]
    MissingRepositoryPath,
    #[error("repository URL contains an unsafe path component")]
    InvalidRemotePath(#[source] PathComponentError),
    #[error("repository URL path contains an encoded path separator")]
    EncodedPathSeparator,
    #[error("repository name must match [A-Za-z0-9_.-]+ and must not be . or ..")]
    InvalidRepositoryName,
    #[error("repository owner must match [A-Za-z0-9_-]+")]
    InvalidOwner,
    #[error("user.name is required when only a repository name is supplied")]
    MissingUserName,
    #[error("user.name must match [A-Za-z0-9_-]+ after whitespace is removed")]
    InvalidUserName,
}

#[derive(Clone, Copy, Debug, Error)]
pub(crate) enum PathComponentError {
    #[error("path component is empty")]
    Empty,
    #[error("path component is .")]
    CurrentDirectory,
    #[error("path component is ..")]
    ParentDirectory,
    #[error("path component contains a path separator or NUL")]
    Separator,
    #[error("path component is not a normal path component on this platform")]
    PlatformPathSyntax,
}

#[cfg(test)]
mod tests {
    use super::{RemoteRepository, RepositoryError};
    use std::path::PathBuf;

    fn assert_repository(
        input: &str,
        user_name: Option<&str>,
        expected_clone_url: &str,
        expected_managed_components: &[&str],
    ) {
        let repository = RemoteRepository::parse(input, user_name).unwrap();
        let managed_components = repository
            .managed_path_components()
            .collect::<Vec<PathBuf>>();

        assert_eq!(
            managed_components,
            expected_managed_components
                .iter()
                .map(PathBuf::from)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            repository.into_clone_url().to_bstring().as_slice(),
            expected_clone_url.as_bytes()
        );
    }

    #[test]
    fn owner_and_repository_shorthand_selects_github_for_clone_and_destination() {
        assert_repository(
            "owner/repository",
            None,
            "https://github.com/owner/repository",
            &["github.com", "owner", "repository"],
        );
    }

    #[test]
    fn repository_shorthand_derives_the_github_owner_from_user_name_without_whitespace() {
        assert_repository(
            "repository",
            Some(" example \t owner\n"),
            "https://github.com/exampleowner/repository",
            &["github.com", "exampleowner", "repository"],
        );
    }

    #[test]
    fn repository_shorthand_requires_a_valid_configured_user_name() {
        assert!(matches!(
            RemoteRepository::parse("repository", None),
            Err(RepositoryError::MissingUserName)
        ));

        for user_name in [" \t\n", "owner/name", "owner.name"] {
            assert!(matches!(
                RemoteRepository::parse("repository", Some(user_name)),
                Err(RepositoryError::InvalidUserName)
            ));
        }
    }

    #[test]
    fn supported_remote_url_forms_preserve_the_clone_target() {
        for input in [
            "http://example.com/owner/repository",
            "https://example.com/owner/repository",
            "ssh://git@example.com/owner/repository",
            "git@example.com:owner/repository",
            "git://example.com/owner/repository",
        ] {
            assert_repository(input, None, input, &["example.com", "owner", "repository"]);
        }
    }

    #[test]
    fn managed_components_preserve_namespaces_and_strip_only_the_final_dot_git_suffix() {
        assert_repository(
            "https://example.com/group.git/subgroup/repository.git",
            None,
            "https://example.com/group.git/subgroup/repository.git",
            &["example.com", "group.git", "subgroup", "repository"],
        );
    }

    #[test]
    fn local_repositories_and_file_urls_are_rejected() {
        for input in ["./owner/repository", "file:///tmp/repository"] {
            assert!(matches!(
                RemoteRepository::parse(input, None),
                Err(RepositoryError::LocalRepository)
            ));
        }
    }

    #[test]
    fn unsupported_url_schemes_are_rejected() {
        assert!(matches!(
            RemoteRepository::parse("ftp://example.com/owner/repository", None),
            Err(RepositoryError::UnsupportedScheme(_))
        ));
    }

    #[test]
    fn a_remote_url_must_identify_a_host_and_repository_path() {
        assert!(RemoteRepository::parse("ssh:///owner/repository", None).is_err());
        assert!(matches!(
            RemoteRepository::parse("https://example.com", None),
            Err(RepositoryError::MissingRepositoryPath)
        ));
    }

    #[test]
    fn unsafe_managed_path_components_are_rejected() {
        for input in [
            "git@example.com:owner//repository",
            "git@example.com:owner/../repository",
        ] {
            assert!(matches!(
                RemoteRepository::parse(input, None),
                Err(RepositoryError::InvalidRemotePath(_))
            ));
        }
    }

    #[test]
    fn encoded_path_separators_are_rejected() {
        for input in [
            "https://example.com/owner%2Frepository",
            "git@example.com:owner%5Crepository",
        ] {
            assert!(matches!(
                RemoteRepository::parse(input, None),
                Err(RepositoryError::EncodedPathSeparator)
            ));
        }
    }

    #[test]
    fn shorthand_owner_and_repository_names_must_be_safe() {
        assert!(matches!(
            RemoteRepository::parse("invalid.owner/repository", None),
            Err(RepositoryError::InvalidOwner)
        ));
        assert!(matches!(
            RemoteRepository::parse("owner/..", None),
            Err(RepositoryError::InvalidRepositoryName)
        ));
    }
}
