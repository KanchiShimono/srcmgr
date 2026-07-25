use std::{error::Error, fmt, path::Path, sync::atomic::AtomicBool};
use thiserror::Error;

pub(crate) fn clone_repository(url: gix::Url, destination: &Path) -> Result<(), CloneError> {
    let create_options = gix::create::Options {
        destination_must_be_empty: Some(true),
        ..Default::default()
    };
    // Unlike gix::prepare_clone(), the default open options do not ask gix to
    // invoke the Git binary to obtain installation configuration.
    let mut prepare = gix::clone::PrepareFetch::new(
        url,
        destination,
        gix::create::Kind::WithWorktree,
        create_options,
        gix::open::Options::default(),
    )
    .map_err(|source| CloneError(CloneErrorKind::Prepare(ErrorSnapshot::capture(&source))))?;

    let interrupted = AtomicBool::new(false);
    let (mut checkout, _) = match prepare.fetch_then_checkout(gix::progress::Discard, &interrupted)
    {
        Ok(prepared) => prepared,
        Err(source) => {
            let source = CloneError(CloneErrorKind::Fetch(ErrorSnapshot::capture(&source)));
            // Disable gix's best-effort Drop cleanup so deletion errors can be
            // observed by the destination transaction.
            drop(prepare.persist());
            return Err(source);
        }
    };

    match checkout.main_worktree(gix::progress::Discard, &interrupted) {
        Ok((repository, _)) => {
            drop(repository);
            Ok(())
        }
        Err(source) => {
            let source = CloneError(CloneErrorKind::Checkout(ErrorSnapshot::capture(&source)));
            drop(checkout.persist());
            Err(source)
        }
    }
}

#[derive(Debug, Error)]
#[error(transparent)]
pub(crate) struct CloneError(CloneErrorKind);

#[derive(Debug, Error)]
enum CloneErrorKind {
    #[error("could not prepare clone: {0}")]
    Prepare(ErrorSnapshot),
    #[error("could not fetch repository: {0}")]
    Fetch(ErrorSnapshot),
    #[error("could not check out repository worktree: {0}")]
    Checkout(ErrorSnapshot),
}

#[derive(Debug)]
struct ErrorSnapshot(String);

impl ErrorSnapshot {
    fn capture(error: &(dyn Error + 'static)) -> Self {
        let mut message = error.to_string();
        let mut source = error.source();
        while let Some(error) = source {
            message.push_str(": ");
            message.push_str(&error.to_string());
            source = error.source();
        }
        Self(message)
    }
}

impl fmt::Display for ErrorSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}
