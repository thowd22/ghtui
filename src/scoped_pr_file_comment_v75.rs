//! A whole changed file, not line1/range/current thread, is the only captured
//! target. This remains an observation until the original worker closes it.
use super::*;
use crate::pr_write_journal::file_comment_history_v75::FileCommentOriginSeedV75;
#[path = "scoped_pr_file_comment_plan_v75.rs"]
mod plan;
pub(crate) use plan::*;
#[path = "scoped_pr_file_comment_form_v75.rs"]
pub mod form;
pub(crate) struct FileCommentIntentV75 {
    pub(crate) original: Arc<ThreadBrowseIntentV1>,
    key: Arc<str>,
    opening_form: code_v4::FormStampV4,
}
impl std::fmt::Debug for FileCommentIntentV75 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FileCommentIntentV75(<actual Files member; no range/permission>)")
    }
}
impl FileCommentIntentV75 {
    pub(crate) fn capture(app: &App) -> Result<Arc<Self>, String> {
        let opening_form = code_v4::FormStampV4::capture(app)?;
        let original = ThreadBrowseIntentV1::capture(app)?;
        let (view, index) = original
            .files_member_v1()
            .ok_or("Select an actual changed file, not Detail/Timeline")?;
        let member = view
            .observed
            .files()
            .get(index)
            .ok_or("Original FILE member unavailable")?;
        let key = FileCommentOriginSeedV75::observed_draft_key(
            original.scope(),
            view.observed.parent(),
            member,
        )
        .map_err(|e| e.to_string())?;
        Ok(Arc::new(Self {
            original,
            key: Arc::from(key),
            opening_form,
        }))
    }
    pub(crate) fn owns(&self, app: &App) -> bool {
        self.original.owns(app)
    }
    pub(crate) fn opening_presentation_owns(&self, app: &App) -> bool {
        self.opening_form.owns(app)
    }
    pub(crate) fn scope(&self) -> &Scope {
        self.original.scope()
    }
    pub(crate) fn draft_key(&self) -> &str {
        &self.key
    }
    pub(crate) fn observed(&self) -> (&Arc<ScopedPrResourceViewV1>, usize) {
        self.original
            .files_member_v1()
            .expect("Closed FILE intent captures genuine member only")
    }
    pub(crate) fn path(&self) -> &str {
        let (v, i) = self.observed();
        &v.observed.files()[i].path
    }
}
