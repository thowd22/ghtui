//! Native-only live App overlap, not a source, authorization or read grant.
//! All arithmetic is borrowed BEFORE identity Arc copies. Never serialized or
//! stored in output receipts; the next explicit phase captures current H anew.
use super::*;
use crate::scoped_actions_workflow_edit_ui_v9::WorkflowFormIdentityV9;
pub(crate) struct WorkflowNativePresentationV9 {
    page: Arc<ScopedActionsPageV1>,
    queued_epoch: u64,
    form: Option<WorkflowFormIdentityV9>,
    bytes: usize,
}
impl WorkflowNativePresentationV9 {
    pub(super) fn capture(app: &App, queued_epoch: u64) -> Result<Arc<Self>, String> {
        let state = app
            .scoped_actions_v1
            .as_ref()
            .filter(|state| current(app, state))
            .ok_or("Actual W9 source/page unavailable")?;
        if app.view != View::Actions
            || queued_epoch != state.epoch.wrapping_add(1)
            || app.loading[View::Actions.index()]
            || app.scoped_actions_work_v1.is_some()
            || app.scoped_actions_owner_v1.is_some()
            || app.scoped_actions_cancel_v1
        {
            return Err("Actual W9 presentation is no longer idle/current".into());
        }
        let bytes = crate::scoped_actions_native_v1::journal_v5::held_presentation_v9(app)?
            .checked_add(std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>())
            .and_then(|n| {
                n.checked_add(if app.form.is_some() {
                    std::mem::size_of::<WorkflowFormIdentityV9>() + 4 * std::mem::size_of::<usize>()
                } else {
                    0
                })
            })
            .filter(|n| *n <= 8 * 1024 * 1024)
            .ok_or("Actual W9 live workspace exceeds original retained bound")?;
        // Parent native gate already ruled out foreign/newer work/overlays.
        // A retained foreign Form cannot be silently omitted from this hold.
        let form = app
            .form
            .as_ref()
            .map(|form| WorkflowFormIdentityV9::capture(form, &state.page))
            .transpose()?;
        Ok(Arc::new(Self {
            page: state.page.clone(),
            queued_epoch,
            form,
            bytes,
        }))
    }
    pub(crate) fn held_bytes(&self) -> usize {
        self.bytes
    }
    pub(crate) fn matches_form(&self, form: &Form) -> bool {
        self.form.as_ref().is_some_and(|stamp| stamp.matches(form))
    }
    pub(crate) fn matches_retirement_form_v9(&self, form: &Form) -> bool {
        self.form
            .as_ref()
            .is_some_and(|stamp| stamp.matches_retirement_v9(form))
    }
    pub(crate) fn matches_intent(&self, intent: &WorkflowEditIntentV9) -> bool {
        Arc::ptr_eq(&self.page, intent.page()) && Arc::ptr_eq(self.page.source(), intent.source())
    }
    pub(crate) fn matches_work(&self, work: &ScopedActionsWorkV1) -> bool {
        self.queued_epoch == work.epoch
            && Arc::ptr_eq(&self.page, &work.page)
            && Arc::ptr_eq(self.page.source(), &work.source)
    }
    pub(super) fn matches_open_app(&self, app: &App) -> bool {
        match (&self.form, &app.form) {
            (None, None) => true,
            (Some(stamp), Some(form)) => stamp.matches(form),
            _ => false,
        }
    }
}
