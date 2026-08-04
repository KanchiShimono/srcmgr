use std::{io, sync::Arc, time::Duration};

use prodash::{
    Progress,
    progress::{DoOrDiscard, key::Level},
    render::{
        self,
        line::{Options, StreamKind},
    },
    tree::{Item, Root},
};

const NORMAL_MAX_LEVEL: Level = 2;
const INITIAL_DELAY: Duration = Duration::from_millis(150);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProgressDetail {
    Normal,
    Verbose,
}

impl From<bool> for ProgressDetail {
    fn from(verbose: bool) -> Self {
        if verbose { Self::Verbose } else { Self::Normal }
    }
}

enum RendererPlan {
    Hidden,
    Live(Options),
}

impl RendererPlan {
    fn for_stderr(detail: ProgressDetail) -> Self {
        let options = Options::default().auto_configure(StreamKind::Stderr);
        Self::from_options(detail, options)
    }

    fn from_options(detail: ProgressDetail, mut options: Options) -> Self {
        // Do not start the line renderer for redirected output. Its shutdown
        // sequence emits terminal control characters even when live rendering
        // is disabled in its options.
        if !options.output_is_terminal {
            return Self::Hidden;
        }

        options.level_filter = match detail {
            ProgressDetail::Normal => Some(1..=NORMAL_MAX_LEVEL),
            ProgressDetail::Verbose => None,
        };
        options.initial_delay = Some(INITIAL_DELAY);
        options.frames_per_second = 6.0;
        options.throughput = true;
        // Cursor hiding requires signal handling to reliably restore it. The
        // line renderer remains readable without taking ownership of it.
        options.hide_cursor = false;

        Self::Live(options)
    }
}

pub(crate) struct ConsoleProgress(RendererPlan);

impl ConsoleProgress {
    pub(crate) fn for_stderr(detail: ProgressDetail) -> Self {
        Self(RendererPlan::for_stderr(detail))
    }

    pub(crate) fn run<T, E>(
        self,
        label: impl Into<String>,
        operation: impl FnOnce(&mut DoOrDiscard<Item>) -> Result<T, E>,
    ) -> Result<T, E> {
        match self.0 {
            RendererPlan::Hidden => {
                let mut progress = DoOrDiscard::<Item>::from(None);
                operation(&mut progress)
            }
            RendererPlan::Live(options) => {
                let root = Root::new();
                let label = label.into();
                let task = root.add_child(label.clone());
                let mut progress = DoOrDiscard::from(Some(task));
                // The task must exist before the renderer starts to avoid its
                // empty-tree startup race.
                let renderer = render::line(io::stderr(), Arc::downgrade(&root), options);

                let result = operation(&mut progress);
                // Operations such as gix fetch may repurpose the parent item
                // and change its name while reporting nested work.
                Progress::set_name(&mut progress, label);
                match &result {
                    Ok(_) => Progress::done(&progress, "completed".to_owned()),
                    Err(_) => Progress::fail(&progress, "failed".to_owned()),
                }

                // Keep the root task alive through the final redraw. Renderer
                // failures are best-effort and must not replace the operation's
                // result.
                renderer.shutdown_and_wait();
                result
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ConsoleProgress, INITIAL_DELAY, ProgressDetail, RendererPlan};
    use prodash::{NestedProgress, render::line::Options};
    use std::sync::Arc;

    fn options(output_is_terminal: bool) -> Options {
        Options {
            output_is_terminal,
            ..Default::default()
        }
    }

    #[test]
    fn parses_verbose_flag_into_a_detail_level() {
        assert_eq!(ProgressDetail::from(false), ProgressDetail::Normal);
        assert_eq!(ProgressDetail::from(true), ProgressDetail::Verbose);
    }

    #[test]
    fn redirected_output_never_starts_a_renderer() {
        for detail in [ProgressDetail::Normal, ProgressDetail::Verbose] {
            assert!(matches!(
                RendererPlan::from_options(detail, options(false)),
                RendererPlan::Hidden
            ));
        }
    }

    #[test]
    fn normal_progress_limits_detail_and_configures_live_rendering() {
        let RendererPlan::Live(options) =
            RendererPlan::from_options(ProgressDetail::Normal, options(true))
        else {
            panic!("terminal output must produce a live renderer");
        };

        assert_eq!(options.level_filter, Some(1..=2));
        assert_eq!(options.initial_delay, Some(INITIAL_DELAY));
        assert_eq!(options.frames_per_second, 6.0);
        assert!(options.throughput);
        assert!(!options.hide_cursor);
    }

    #[test]
    fn verbose_progress_shows_every_detail_level() {
        let RendererPlan::Live(options) =
            RendererPlan::from_options(ProgressDetail::Verbose, options(true))
        else {
            panic!("terminal output must produce a live renderer");
        };

        assert_eq!(options.level_filter, None);
    }

    #[test]
    fn hidden_progress_returns_success_without_changing_it() {
        let progress = ConsoleProgress(RendererPlan::Hidden);

        let result = progress.run("task", |progress| {
            let _child = progress.add_child("child");
            Ok::<_, ()>(42)
        });

        assert_eq!(result, Ok(42));
    }

    #[test]
    fn hidden_progress_returns_the_same_error_value() {
        let progress = ConsoleProgress(RendererPlan::Hidden);
        let expected = Arc::new("sentinel");

        let result = progress.run("task", |_| Err::<(), _>(Arc::clone(&expected)));
        let actual = result.unwrap_err();

        assert!(Arc::ptr_eq(&actual, &expected));
    }
}
