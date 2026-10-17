//! Drop only the same dismissed, read-only Base/Members history presentation.
//! This synchronous Close proof is neither a draft ticket nor read/write authority.
use super::*;

enum HistoryIdentityV296 {
    Base(metadata_base_v32::history::HistoryCloseIdentityV296),
    Members(metadata_members_v48::history::HistoryCloseIdentityV296),
}
impl HistoryIdentityV296 {
    fn matches(&self, form: &Form) -> bool {
        match self {
            Self::Base(identity) => identity.matches_form(form),
            Self::Members(identity) => identity.matches_form(form),
        }
    }
    fn owns_original(&self, app: &App) -> bool {
        match self {
            Self::Base(identity) => identity.owns_original(app),
            Self::Members(identity) => identity.owns_original(app),
        }
    }
}
pub(crate) struct HistoryCloseProofV296 {
    presentation: Arc<()>,
    identity: HistoryIdentityV296,
}
fn idle(app: &App) -> bool {
    app.view == View::Inbox
        && !app.loading[View::Inbox.index()]
        && !crate::scoped_pr_collaboration_native_v17::newer_intent(app)
        && app.scoped_pr_collaboration_work_v17.is_none()
        && app.scoped_pr_collaboration_owner_v17.is_none()
        && !app.scoped_pr_collaboration_cancel_v17
        && app.scoped_inline_owner_v1.is_none()
        && app.scoped_threads_owner_v1.is_none()
        && app.scoped_reviews_owner_v2.is_none()
        && app.pr_native_diff_owner_v1.is_none()
        && app.personal_pr_entry_owner_v13.is_none()
        && app.scoped_actions_owner_v1.is_none()
}
fn read_only(form: &Form) -> bool {
    form.fields.is_empty()
        && !form.dirty
        && !form.submitting
        && !form.submitted
        && !form.preview_requested
        && !form.submit_requested
        && !form.editor_requested
        && form.publication.is_none()
        && form.actions_publication.is_none()
        && form.picker.is_none()
        && form.picker_requested.is_none()
        && form.metadata_picker.is_none()
        && metadata_base_v32::picker(form).is_none()
}
pub(crate) fn capture(app: &App) -> Option<HistoryCloseProofV296> {
    if !idle(app) || app.overlay != Some(Overlay::Form) || app.form_dismissed {
        return None;
    }
    let form = app.form.as_ref()?;
    if !read_only(form) || !form.preview {
        return None;
    }
    let FormPurpose::PrCollaborationV17(state) = &form.purpose else {
        return None;
    };
    if !state.original().owns_presentation(app) {
        return None;
    }
    let identity = if let Some(identity) = metadata_base_v32::history::capture_close_v296(form) {
        HistoryIdentityV296::Base(identity)
    } else {
        HistoryIdentityV296::Members(metadata_members_v48::history::capture_close_v296(form)?)
    };
    if !identity.owns_original(app) {
        return None;
    }
    Some(HistoryCloseProofV296 {
        presentation: Arc::clone(form.presentation_v4().0),
        identity,
    })
}
pub(crate) fn form_closed(app: &mut App, proof: HistoryCloseProofV296) -> bool {
    if !idle(app) || app.overlay.is_some() || !app.form_dismissed {
        return false;
    }
    let Some(form) = app.form.as_ref() else {
        return false;
    };
    if !read_only(form)
        || form.preview
        || form.preview_display_v1.document.is_some()
        || !Arc::ptr_eq(form.presentation_v4().0, &proof.presentation)
        || !proof.identity.matches(form)
        || !proof.identity.owns_original(app)
    {
        return false;
    }
    app.form = None;
    app.hits.clear();
    true
}
