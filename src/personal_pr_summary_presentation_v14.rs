//! Mechanical identity of an ACTUAL cached Personal Summary presentation.
//! This is NOT a source, access/Files receipt, history witness or permission.
//! The worker still proves original UID/repo/raw parent/DB/ref and closes it.
use super::*;

pub(crate) struct PersonalPrSummaryPresentationV14 {
    opening: Arc<PersonalPrEntryWorkV13>,
}
impl std::fmt::Debug for PersonalPrSummaryPresentationV14 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "PersonalPrSummaryPresentationV14(<actual cached page ticket; no source/permission>)",
        )
    }
}
impl PersonalPrSummaryPresentationV14 {
    /// Cached, bounded, no I/O. Same actual capture as canonical Files, but no
    /// work/source is queued or installed and the nonClone page stays on App.
    pub(crate) fn capture(app: &App, database: &Path) -> Result<Arc<Self>, String> {
        if app
            .github
            .as_ref()
            .and_then(|g| g.pr.as_ref())
            .is_none_or(|page| page.tab != PrTab::Summary)
        {
            return Err("Open the actual Personal Summary first; no Files substitute".into());
        }
        let result = Arc::new(Self {
            opening: PersonalPrEntryWorkV13::capture(app, database)?,
        });
        if !result.matches_capture(app) {
            return Err("Original Summary capture changed; actual page retained".into());
        }
        Ok(result)
    }
    pub(crate) fn parent(&self) -> &Arc<PrParentIntentV13> {
        self.opening.parent()
    }
    pub(crate) fn scope(&self) -> &Scope {
        &self.opening.original.numeric_scope
    }
    pub(crate) fn database(&self) -> &Path {
        &self.opening.database
    }
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        self.opening.cancellation()
    }
    /// INITIAL queued request clock only, never an expiring display lease or
    /// permission to renew an already-entered worker phase after thirty seconds.
    pub(crate) fn deadline(&self) -> Instant {
        self.opening.deadline()
    }
    pub(crate) fn matches_capture(&self, app: &App) -> bool {
        self.opening.matches_capture(app)
    }
    /// Source/page identity only. The owned caller must separately match its
    /// actual overlay/Form/browser ticket, row, operation token and generation.
    /// History controls change their own focus without changing this old page.
    pub(crate) fn owns_presentation(&self, app: &App) -> bool {
        self.same_presentation(app)
            && app
                .github
                .as_ref()
                .is_some_and(|github| github.scope().ok().as_ref() == Some(self.scope()))
            && app.selected == self.opening.original.selected_row
            && app.scroll == self.opening.original.scroll
    }
    /// No numeric-account assertion, deliberately: authentic current account
    /// loss must hide ONLY this exact original private surface even if its old
    /// Background physical fence was independently retired. Not read authority.
    pub(crate) fn same_presentation(&self, app: &App) -> bool {
        self.opening.original.current_pointers(app)
    }
    pub(super) fn retained_same_v19(
        &self,
        app: &App,
        github: &crate::github_workspace::GithubWorkspace,
    ) -> bool {
        self.opening.original.retained_pointers_v19(app, github)
    }
    pub(super) fn retained_owns_v19(
        &self,
        app: &App,
        github: &crate::github_workspace::GithubWorkspace,
    ) -> bool {
        self.retained_same_v19(app, github)
            && github.scope().ok().as_ref() == Some(self.scope())
            && app.selected == self.opening.original.selected_row
            && app.scroll == self.opening.original.scroll
    }
    /// No Background slot/token/generation/loading/status or shared-avatar-cache
    /// cleanup here. Caller retires ONLY its matching local history/Form ticket.
    pub(crate) fn retire_account_mismatch(&self, app: &mut App) -> bool {
        if !self.same_presentation(app)
            || app
                .github
                .as_ref()
                .is_none_or(|github| github.scope().ok().as_ref() == Some(self.scope()))
        {
            return false;
        }
        retire_original_presentation(app);
        true
    }
}

#[cfg(test)]
#[path = "personal_pr_summary_presentation_v14_tests.rs"]
mod tests;
