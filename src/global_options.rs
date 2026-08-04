use crate::progress::ProgressDetail;

#[derive(Debug)]
pub(crate) struct GlobalOptions {
    progress_detail: ProgressDetail,
}

impl GlobalOptions {
    pub(crate) const fn new(progress_detail: ProgressDetail) -> Self {
        Self { progress_detail }
    }

    pub(crate) const fn progress_detail(&self) -> ProgressDetail {
        self.progress_detail
    }
}
