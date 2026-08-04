use crate::{
    canonical_dir::CanonicalDir,
    config::{Config, ConfigError},
    global_options::GlobalOptions,
    non_empty_vec::NonEmptyVec,
    path_expansion::HomeDirectory,
};
use clap::Args;
use gix::discover::path::{self as git_path, from_gitdir_file::Error as GitFileError};
use std::{
    fs,
    io::{self, ErrorKind, Write},
    path::{Path, PathBuf},
};
use thiserror::Error;
use walkdir::WalkDir;

#[derive(Debug, Args)]
pub(crate) struct ListArgs {
    /// Print paths relative to each management root
    #[arg(short = 'r', long)]
    relative: bool,

    /// Include repositories nested inside another repository
    #[arg(short = 'n', long)]
    nested: bool,
}

pub(crate) fn run(args: &ListArgs, _global: &GlobalOptions) -> anyhow::Result<()> {
    let home = HomeDirectory::discover()
        .map_err(ConfigError::from)
        .map_err(ListError::Config)?;
    let config = Config::load(&home).map_err(ListError::Config)?;
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    run_with_io(args, config.roots(), &mut stdout, &mut stderr)?;
    Ok(())
}

fn run_with_io(
    args: &ListArgs,
    roots: &NonEmptyVec<CanonicalDir>,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<(), ListError> {
    let errors = scan(args, roots, |path| {
        writeln!(stdout, "{}", path.display()).map_err(ListError::WriteRepositoryPath)?;
        stdout.flush().map_err(ListError::FlushRepositoryPath)
    })?;

    for error in &errors {
        writeln!(stderr, "{error}").map_err(ListError::WriteDiagnostic)?;
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(ListError::ScanFailed {
            count: errors.len(),
        })
    }
}

fn scan<E>(
    args: &ListArgs,
    roots: &NonEmptyVec<CanonicalDir>,
    mut found: impl FnMut(&Path) -> Result<(), E>,
) -> Result<Vec<ListDiagnostic>, E> {
    let mut errors = Vec::new();

    for root in roots.iter() {
        let root = root.as_path();
        // Filesystem state may have changed since Config was loaded.
        match fs::symlink_metadata(root) {
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => {
                errors.push(ListDiagnostic::ManagementRootNotDirectory {
                    path: root.to_owned(),
                });
                continue;
            }
            Err(source) => {
                errors.push(ListDiagnostic::InspectManagementRoot {
                    path: root.to_owned(),
                    source,
                });
                continue;
            }
        }

        let mut entries = WalkDir::new(root)
            .follow_links(false)
            .sort_by_file_name()
            .into_iter();

        while let Some(entry) = entries.next() {
            let entry = match entry {
                Ok(entry) => entry,
                Err(source) => {
                    let path = source.path().unwrap_or(root).to_owned();
                    errors.push(ListDiagnostic::Walk { path, source });
                    continue;
                }
            };

            if !entry.file_type().is_dir() || !is_repository(entry.path(), &mut errors) {
                continue;
            }

            let path = if args.relative {
                let path = entry
                    .path()
                    .strip_prefix(root)
                    .expect("walked paths are below their root");
                if path.as_os_str().is_empty() {
                    PathBuf::from(".")
                } else {
                    path.to_owned()
                }
            } else {
                entry.path().to_owned()
            };
            found(&path)?;

            if !args.nested {
                entries.skip_current_dir();
            }
        }
    }

    Ok(errors)
}

fn is_repository(path: &Path, errors: &mut Vec<ListDiagnostic>) -> bool {
    let mut found = match is_git_repository(path) {
        Ok(found) => found,
        Err(source) => {
            errors.push(ListDiagnostic::Git {
                path: path.to_owned(),
                source,
            });
            false
        }
    };

    for name in [".hg", ".svn"] {
        let marker = path.join(name);
        match fs::metadata(&marker) {
            Ok(metadata) => found |= metadata.is_dir(),
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(source) => errors.push(ListDiagnostic::InspectRepositoryMarker {
                path: marker,
                source,
            }),
        }
    }

    found
}

fn is_git_repository(path: &Path) -> Result<bool, GitRepositoryError> {
    let marker = path.join(".git");
    let metadata = match fs::metadata(&marker) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(source) => {
            return Err(GitRepositoryError::InspectMarker {
                path: marker,
                source,
            });
        }
    };

    if metadata.is_dir() {
        return Ok(true);
    }
    if !metadata.is_file() {
        return Err(GitRepositoryError::InvalidMarkerType { path: marker });
    }

    let git_dir = git_path::from_gitdir_file(&marker).map_err(|source| {
        GitRepositoryError::InvalidGitFile {
            path: marker.clone(),
            source,
        }
    })?;
    match fs::metadata(&git_dir) {
        Ok(metadata) if metadata.is_dir() => Ok(true),
        Ok(_) => Err(GitRepositoryError::GitDirectoryNotDirectory { path: git_dir }),
        Err(source) => Err(GitRepositoryError::InspectGitDirectory {
            path: git_dir,
            git_file: marker,
            source,
        }),
    }
}

#[derive(Debug, Error)]
enum ListError {
    #[error("failed to load configuration")]
    Config(#[source] ConfigError),
    #[error("could not write repository path")]
    WriteRepositoryPath(#[source] io::Error),
    #[error("could not flush repository path")]
    FlushRepositoryPath(#[source] io::Error),
    #[error("could not write diagnostic")]
    WriteDiagnostic(#[source] io::Error),
    #[error("encountered {count} error(s) while listing repositories")]
    ScanFailed { count: usize },
}

#[derive(Debug, Error)]
enum ListDiagnostic {
    #[error("{}: management root is not a directory", path.display())]
    ManagementRootNotDirectory { path: PathBuf },
    #[error(
        "{}: could not inspect management root: {source}",
        path.display()
    )]
    InspectManagementRoot {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{}: {source}", path.display())]
    Walk {
        path: PathBuf,
        #[source]
        source: walkdir::Error,
    },
    #[error("{}: {source}", path.display())]
    Git {
        path: PathBuf,
        #[source]
        source: GitRepositoryError,
    },
    #[error("{}: {source}", path.display())]
    InspectRepositoryMarker {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

#[derive(Debug, Error)]
enum GitRepositoryError {
    #[error("could not inspect {}: {source}", path.display())]
    InspectMarker {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{} is not a directory or regular file", path.display())]
    InvalidMarkerType { path: PathBuf },
    #[error("invalid Git file {}: {source}", path.display())]
    InvalidGitFile {
        path: PathBuf,
        #[source]
        source: GitFileError,
    },
    #[error("{} is not a directory", path.display())]
    GitDirectoryNotDirectory { path: PathBuf },
    #[error(
        "could not inspect {} referenced by {}: {source}",
        path.display(),
        git_file.display()
    )]
    InspectGitDirectory {
        path: PathBuf,
        git_file: PathBuf,
        #[source]
        source: io::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::{GitRepositoryError, ListArgs, ListDiagnostic, ListError};
    use crate::{canonical_dir::CanonicalDir, non_empty_vec::NonEmptyVec};
    use clap::Parser;
    use std::{
        convert::Infallible,
        fs,
        io::{self, Error, ErrorKind, Write},
        path::{Path, PathBuf},
        slice,
    };

    const DEFAULT: ListArgs = ListArgs {
        relative: false,
        nested: false,
    };
    const RELATIVE: ListArgs = ListArgs {
        relative: true,
        nested: false,
    };
    const RELATIVE_NESTED: ListArgs = ListArgs {
        relative: true,
        nested: true,
    };

    #[derive(Parser)]
    struct Options {
        #[command(flatten)]
        list: ListArgs,
    }

    fn parse(options: &[&str]) -> ListArgs {
        Options::parse_from(["list"].into_iter().chain(options.iter().copied())).list
    }

    fn repository(path: &Path, marker: &str) {
        fs::create_dir_all(path.join(marker)).unwrap();
    }

    fn canonical_roots(roots: &[PathBuf]) -> NonEmptyVec<CanonicalDir> {
        let roots = roots
            .iter()
            .cloned()
            .map(|root| CanonicalDir::try_from(root).unwrap())
            .collect::<Vec<_>>();
        NonEmptyVec::try_from(roots).unwrap()
    }

    fn invoke(args: &ListArgs, roots: &[PathBuf]) -> (Result<(), ListError>, String, String) {
        let roots = canonical_roots(roots);
        invoke_with_roots(args, &roots)
    }

    fn invoke_with_roots(
        args: &ListArgs,
        roots: &NonEmptyVec<CanonicalDir>,
    ) -> (Result<(), ListError>, String, String) {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let result = super::run_with_io(args, roots, &mut stdout, &mut stderr);
        (
            result,
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    }

    fn scan_paths(args: &ListArgs, roots: &[PathBuf]) -> (Vec<PathBuf>, Vec<ListDiagnostic>) {
        let roots = canonical_roots(roots);
        scan_with_roots(args, &roots)
    }

    fn scan_with_roots(
        args: &ListArgs,
        roots: &NonEmptyVec<CanonicalDir>,
    ) -> (Vec<PathBuf>, Vec<ListDiagnostic>) {
        let mut paths = Vec::new();
        let errors = super::scan(args, roots, |path| {
            paths.push(path.to_owned());
            Ok::<(), Infallible>(())
        })
        .unwrap();
        (paths, errors)
    }

    struct BufferedOutput<F> {
        pending: Vec<u8>,
        visible: Vec<u8>,
        lines: usize,
        action: F,
    }

    impl<F: FnMut(usize) -> io::Result<()>> Write for BufferedOutput<F> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.pending.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.visible.append(&mut self.pending);
            let lines = self.visible.iter().filter(|byte| **byte == b'\n').count();
            while self.lines < lines {
                self.lines += 1;
                (self.action)(self.lines)?;
            }
            Ok(())
        }
    }

    struct WriteFailure;

    impl Write for WriteFailure {
        fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
            Err(Error::new(ErrorKind::BrokenPipe, "injected write failure"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn accepts_relative_option() {
        for option in ["--relative", "-r"] {
            let args = parse(&[option]);
            assert!(args.relative);
            assert!(!args.nested);
        }
    }

    #[test]
    fn accepts_nested_option() {
        for option in ["--nested", "-n"] {
            let args = parse(&[option]);
            assert!(!args.relative);
            assert!(args.nested);
        }
    }

    #[test]
    fn uses_default_options() {
        let args = parse(&[]);

        assert!(!args.relative && !args.nested);
    }

    #[test]
    fn accepts_both_options() {
        let args = parse(&["--relative", "--nested"]);

        assert!(args.relative && args.nested);
    }

    #[cfg(unix)]
    #[test]
    fn ignores_directory_symlinks_below_roots() {
        use std::os::unix::fs as unix_fs;

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let outside = temp.path().join("outside");
        repository(&root.join("direct"), ".git");
        repository(&outside.join("linked"), ".git");
        unix_fs::symlink(&outside, root.join("directory-link")).unwrap();

        let (paths, errors) = scan_paths(&RELATIVE, &[root]);

        assert_eq!(paths, [PathBuf::from("direct")]);
        assert!(errors.is_empty());
    }

    #[test]
    fn recognizes_git_hg_and_svn_directories() {
        for marker in [".git", ".hg", ".svn"] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("root");
            repository(&root.join("repository"), marker);

            let (paths, errors) = scan_paths(&RELATIVE, &[root]);

            assert_eq!(paths, [PathBuf::from("repository")]);
            assert!(errors.is_empty());
        }
    }

    #[test]
    fn ignores_hg_and_svn_files() {
        for marker in [".hg", ".svn"] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("root");
            fs::create_dir_all(root.join("candidate")).unwrap();
            fs::write(root.join("candidate").join(marker), "").unwrap();

            let (paths, errors) = scan_paths(&RELATIVE, &[root]);

            assert!(paths.is_empty());
            assert!(errors.is_empty());
        }
    }

    #[test]
    fn recognizes_gitfiles() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let worktree = root.join("worktree");
        let gitdir = root.join("gitdir");
        fs::create_dir_all(&worktree).unwrap();
        fs::create_dir_all(&gitdir).unwrap();
        fs::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", gitdir.display()),
        )
        .unwrap();

        let (paths, errors) = scan_paths(&RELATIVE, &[root]);

        assert_eq!(paths, [PathBuf::from("worktree")]);
        assert!(errors.is_empty());
    }

    #[test]
    fn resolves_gitdirs_relative_to_gitfile_parents() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        fs::create_dir_all(root.join("worktree")).unwrap();
        fs::create_dir_all(root.join("gitdir")).unwrap();
        fs::write(root.join("worktree").join(".git"), "gitdir: ../gitdir\n").unwrap();

        let (paths, errors) = scan_paths(&RELATIVE, &[root]);

        assert_eq!(paths, [PathBuf::from("worktree")]);
        assert!(errors.is_empty());
    }

    #[test]
    fn requires_gitfile_targets_to_exist() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let worktree = root.join("worktree");
        fs::create_dir_all(&worktree).unwrap();
        fs::write(worktree.join(".git"), "gitdir: ../missing\n").unwrap();
        let expected_worktree = fs::canonicalize(&root).unwrap().join("worktree");

        let (paths, errors) = scan_paths(&RELATIVE, &[root]);

        assert!(paths.is_empty());
        assert!(matches!(
            errors.as_slice(),
            [ListDiagnostic::Git {
                path,
                source: GitRepositoryError::InspectGitDirectory { .. },
            }] if path == &expected_worktree
        ));
    }

    #[test]
    fn scans_roots_in_configuration_order() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        repository(&first.join("from-first"), ".git");
        repository(&second.join("from-second"), ".git");

        let (paths, errors) = scan_paths(&RELATIVE, &[second, first]);

        assert_eq!(paths, ["from-second", "from-first"].map(PathBuf::from));
        assert!(errors.is_empty());
    }

    #[test]
    fn scans_paths_in_lexicographic_order() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        repository(&root.join("zeta"), ".git");
        repository(&root.join("alpha").join("deep"), ".git");

        let (paths, errors) = scan_paths(&RELATIVE, &[root]);

        assert_eq!(
            paths,
            [Path::new("alpha").join("deep"), PathBuf::from("zeta")]
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn stops_at_repositories_by_default() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        repository(&root.join("outer"), ".git");
        repository(&root.join("outer").join("inner"), ".git");

        let (paths, errors) = scan_paths(&RELATIVE, &[root]);

        assert_eq!(paths, [PathBuf::from("outer")]);
        assert!(errors.is_empty());
    }

    #[test]
    fn scans_nested_repositories() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        repository(&root.join("outer"), ".git");
        repository(&root.join("outer").join("inner"), ".git");

        let (paths, errors) = scan_paths(&RELATIVE_NESTED, &[root]);

        assert_eq!(
            paths,
            [PathBuf::from("outer"), Path::new("outer").join("inner")]
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn writes_repositories_as_they_are_found() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        repository(&root.join("alpha"), ".git");
        let middle = root.join("middle");
        let sentinel = root.join("zz-sentinel");
        fs::create_dir_all(&middle).unwrap();
        repository(&sentinel, ".git");
        let mut stdout = BufferedOutput {
            pending: Vec::new(),
            visible: Vec::new(),
            lines: 0,
            action: move |line| match line {
                1 => fs::create_dir(middle.join(".git")),
                2 => fs::remove_dir(sentinel.join(".git")),
                _ => Ok(()),
            },
        };
        let mut stderr = Vec::new();
        let roots = canonical_roots(slice::from_ref(&root));

        super::run_with_io(&RELATIVE, &roots, &mut stdout, &mut stderr).unwrap();

        assert_eq!(
            String::from_utf8(stdout.visible).unwrap(),
            "alpha\nmiddle\n"
        );
    }

    #[test]
    fn classifies_repository_path_write_errors() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        repository(&root.join("repository"), ".git");
        let roots = canonical_roots(slice::from_ref(&root));
        let mut stdout = WriteFailure;
        let mut stderr = Vec::new();

        let result = super::run_with_io(&RELATIVE, &roots, &mut stdout, &mut stderr);

        assert!(matches!(
            result,
            Err(ListError::WriteRepositoryPath(source))
                if source.kind() == ErrorKind::BrokenPipe
        ));
    }

    #[test]
    fn classifies_repository_path_flush_errors() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        repository(&root.join("repository"), ".git");
        let roots = canonical_roots(slice::from_ref(&root));
        let mut stdout = BufferedOutput {
            pending: Vec::new(),
            visible: Vec::new(),
            lines: 0,
            action: |_| Err(Error::new(ErrorKind::BrokenPipe, "injected flush failure")),
        };
        let mut stderr = Vec::new();

        let result = super::run_with_io(&RELATIVE, &roots, &mut stdout, &mut stderr);

        assert!(matches!(
            result,
            Err(ListError::FlushRepositoryPath(source))
                if source.kind() == ErrorKind::BrokenPipe
        ));
    }

    #[test]
    fn classifies_diagnostic_write_errors() {
        let temp = tempfile::tempdir().unwrap();
        let removed = temp.path().join("removed");
        fs::create_dir(&removed).unwrap();
        let roots = canonical_roots(slice::from_ref(&removed));
        fs::remove_dir(&removed).unwrap();
        let mut stdout = Vec::new();
        let mut stderr = WriteFailure;

        let result = super::run_with_io(&DEFAULT, &roots, &mut stdout, &mut stderr);

        assert!(matches!(
            result,
            Err(ListError::WriteDiagnostic(source))
                if source.kind() == ErrorKind::BrokenPipe
        ));
    }

    #[test]
    fn writes_one_line_per_repository() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        repository(&root.join("alpha"), ".git");
        repository(&root.join("beta"), ".git");

        let (result, stdout, _) = invoke(&RELATIVE, &[root]);

        result.unwrap();
        let mut lines = stdout.lines().collect::<Vec<_>>();
        lines.sort_unstable();
        assert_eq!(lines, ["alpha", "beta"]);
        assert_eq!(stdout.matches('\n').count(), 2);
    }

    #[test]
    fn prints_absolute_paths_by_default() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let repository_path = root.join("repository");
        repository(&repository_path, ".git");

        let (result, stdout, _) = invoke(&DEFAULT, &[root]);

        result.unwrap();
        assert_eq!(
            stdout,
            format!("{}\n", fs::canonicalize(repository_path).unwrap().display())
        );
    }

    #[test]
    fn prints_paths_relative_to_each_root() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        let relative = PathBuf::from("repository");
        repository(&first.join(&relative), ".git");
        repository(&second.join(&relative), ".git");

        let (result, stdout, _) = invoke(&RELATIVE, &[first, second]);

        result.unwrap();
        assert_eq!(stdout, format!("{0}\n{0}\n", relative.display()));
    }

    #[test]
    fn writes_path_qualified_diagnostics_only_to_stderr() {
        let temp = tempfile::tempdir().unwrap();
        let removed = temp.path().join("removed");
        fs::create_dir(&removed).unwrap();
        let roots = canonical_roots(slice::from_ref(&removed));
        let canonical = roots.iter().next().unwrap().as_path().to_owned();
        fs::remove_dir(&removed).unwrap();

        let (_, stdout, stderr) = invoke_with_roots(&DEFAULT, &roots);

        assert!(stdout.is_empty());
        assert!(stderr.contains(&canonical.display().to_string()));
    }

    #[test]
    fn continues_after_roots_are_removed_after_configuration_loads() {
        let temp = tempfile::tempdir().unwrap();
        let removed = temp.path().join("removed");
        let root = temp.path().join("root");
        fs::create_dir(&removed).unwrap();
        repository(&root.join("valid"), ".git");
        let roots = canonical_roots(&[removed.clone(), root]);
        let expected_removed = roots.iter().next().unwrap().as_path().to_owned();
        fs::remove_dir(&removed).unwrap();

        let (paths, errors) = scan_with_roots(&RELATIVE, &roots);

        assert_eq!(paths, [PathBuf::from("valid")]);
        assert!(matches!(
            errors.as_slice(),
            [ListDiagnostic::InspectManagementRoot { path, .. }]
                if path == &expected_removed
        ));
    }

    #[test]
    fn records_roots_replaced_with_files_after_configuration_loads() {
        let temp = tempfile::tempdir().unwrap();
        let replaced = temp.path().join("replaced");
        fs::create_dir(&replaced).unwrap();
        let roots = canonical_roots(slice::from_ref(&replaced));
        let expected_replaced = roots.first().as_path().to_owned();
        fs::remove_dir(&replaced).unwrap();
        fs::write(&replaced, "not a directory").unwrap();

        let (paths, errors) = scan_with_roots(&DEFAULT, &roots);

        assert!(paths.is_empty());
        assert!(matches!(
            errors.as_slice(),
            [ListDiagnostic::ManagementRootNotDirectory { path }]
                if path == &expected_replaced
        ));
    }

    #[cfg(unix)]
    #[test]
    fn records_roots_replaced_with_symlinks_after_configuration_loads() {
        use std::os::unix::fs as unix_fs;

        let temp = tempfile::tempdir().unwrap();
        let replaced = temp.path().join("replaced");
        let outside = temp.path().join("outside");
        fs::create_dir(&replaced).unwrap();
        repository(&outside.join("repository"), ".git");
        let roots = canonical_roots(slice::from_ref(&replaced));
        let expected_replaced = roots.first().as_path().to_owned();
        fs::remove_dir(&replaced).unwrap();
        unix_fs::symlink(&outside, &replaced).unwrap();

        let (paths, errors) = scan_with_roots(&RELATIVE, &roots);

        assert!(paths.is_empty());
        assert!(matches!(
            errors.as_slice(),
            [ListDiagnostic::ManagementRootNotDirectory { path }]
                if path == &expected_replaced
        ));
    }

    #[test]
    fn records_invalid_gitfiles_and_continues() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let malformed = root.join("malformed");
        fs::create_dir_all(&malformed).unwrap();
        fs::write(malformed.join(".git"), "not a gitdir").unwrap();
        repository(&root.join("valid"), ".svn");
        let roots = canonical_roots(slice::from_ref(&root));
        let expected_malformed = roots.iter().next().unwrap().as_path().join("malformed");

        let (paths, errors) = scan_with_roots(&RELATIVE, &roots);

        assert_eq!(paths, [PathBuf::from("valid")]);
        assert!(errors.iter().any(|error| matches!(
            error,
            ListDiagnostic::Git {
                path,
                source: GitRepositoryError::InvalidGitFile { .. },
            } if path == &expected_malformed
        )));
    }

    #[test]
    fn records_directory_read_errors_and_continues() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        repository(&root.join("alpha"), ".git");
        let unreadable = root.join("middle-unreadable");
        fs::create_dir_all(unreadable.join("child")).unwrap();
        repository(&root.join("zeta"), ".git");
        let mut repositories = Vec::new();
        let roots = canonical_roots(slice::from_ref(&root));
        let expected_unreadable = roots
            .iter()
            .next()
            .unwrap()
            .as_path()
            .join("middle-unreadable");

        let errors = super::scan(&RELATIVE, &roots, |path| {
            repositories.push(path.to_owned());
            if path == Path::new("alpha") {
                fs::remove_dir_all(&unreadable)?;
            }
            Ok::<(), Error>(())
        })
        .unwrap();

        assert!(repositories.contains(&PathBuf::from("zeta")));
        assert!(errors.iter().any(|error| matches!(
            error,
            ListDiagnostic::Walk { path, .. }
                if path.starts_with(&expected_unreadable)
        )));
    }

    #[test]
    fn fails_after_scanning_with_one_error() {
        let temp = tempfile::tempdir().unwrap();
        let removed = temp.path().join("removed");
        let root = temp.path().join("root");
        fs::create_dir(&removed).unwrap();
        repository(&root.join("valid"), ".git");
        let roots = canonical_roots(&[removed.clone(), root]);
        fs::remove_dir(&removed).unwrap();

        let (result, stdout, _) = invoke_with_roots(&RELATIVE, &roots);

        assert_eq!(stdout, "valid\n");
        assert!(matches!(result, Err(ListError::ScanFailed { count: 1 })));
    }

    #[test]
    fn succeeds_without_repositories() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        fs::create_dir(&root).unwrap();

        let (result, stdout, stderr) = invoke(&DEFAULT, &[root]);

        result.unwrap();
        assert!(stdout.is_empty());
        assert!(stderr.is_empty());
    }
}
