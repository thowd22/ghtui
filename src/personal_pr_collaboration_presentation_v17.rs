//! Actual cached Summary or Conversation presentation. Never a source, account
//! proof, completed parent boundary, or publication permission.
use super::*;

enum TabV17 {
    Summary(Arc<summary_v14::PersonalPrSummaryPresentationV14>),
    Conversation(Arc<PersonalPrEntryWorkV13>),
    Metadata(Arc<PersonalPrEntryWorkV13>),
}
pub(crate) struct PersonalPrCollaborationPresentationV17(TabV17);
impl std::fmt::Debug for PersonalPrCollaborationPresentationV17 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "PersonalPrCollaborationPresentationV17(<actual cached tab/page ticket; no source>)",
        )
    }
}
impl PersonalPrCollaborationPresentationV17 {
    pub(crate) fn capture(app: &App, database: &Path) -> Result<Arc<Self>, String> {
        let tab = app
            .github
            .as_ref()
            .and_then(|github| github.pr.as_ref())
            .map(|page| page.tab)
            .ok_or("Open the actual personal PR first")?;
        let actual = match tab {
            PrTab::Summary => TabV17::Summary(
                summary_v14::PersonalPrSummaryPresentationV14::capture(app, database)?,
            ),
            PrTab::Conversation => {
                TabV17::Conversation(PersonalPrEntryWorkV13::capture(app, database)?)
            }
            _ => {
                return Err(
                    "Open actual Summary or Conversation; no substitute tab or Files source".into(),
                )
            }
        };
        let captured = Arc::new(Self(actual));
        if !captured.matches_capture(app) {
            return Err("Original actual collaboration tab changed during cached capture".into());
        }
        Ok(captured)
    }
    /// Closed metadata presentation only. The actual cached page/tab tuple
    /// stays on App; this descriptor grants neither read nor write authority.
    pub(crate) fn capture_metadata_v51(app: &App, database: &Path) -> Result<Arc<Self>, String> {
        if app
            .github
            .as_ref()
            .and_then(|g| g.pr.as_ref())
            .map(|p| p.tab)
            != Some(PrTab::Metadata)
        {
            return Err("Metadata requires the actual Metadata tab; no tab substitution".into());
        }
        let captured = Arc::new(Self(TabV17::Metadata(PersonalPrEntryWorkV13::capture(
            app, database,
        )?)));
        if !captured.matches_capture(app) {
            return Err("Original actual Metadata presentation changed during capture".into());
        }
        Ok(captured)
    }
    #[cfg(test)]
    pub(crate) fn tab(&self) -> PrTab {
        self.refresh_tab_v19()
    }
    pub(crate) fn refresh_tab_v19(&self) -> PrTab {
        match &self.0 {
            TabV17::Summary(_) => PrTab::Summary,
            TabV17::Conversation(_) => PrTab::Conversation,
            TabV17::Metadata(_) => PrTab::Metadata,
        }
    }
    pub(crate) fn parent(&self) -> &Arc<PrParentIntentV13> {
        match &self.0 {
            TabV17::Summary(v) => v.parent(),
            TabV17::Conversation(v) | TabV17::Metadata(v) => v.parent(),
        }
    }
    pub(crate) fn scope(&self) -> &Scope {
        match &self.0 {
            TabV17::Summary(v) => v.scope(),
            TabV17::Conversation(v) | TabV17::Metadata(v) => &v.original.numeric_scope,
        }
    }
    pub(crate) fn database(&self) -> &Path {
        match &self.0 {
            TabV17::Summary(v) => v.database(),
            TabV17::Conversation(v) | TabV17::Metadata(v) => &v.database,
        }
    }
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        match &self.0 {
            TabV17::Summary(v) => v.cancellation(),
            TabV17::Conversation(v) | TabV17::Metadata(v) => v.cancellation(),
        }
    }
    pub(crate) fn deadline(&self) -> Instant {
        match &self.0 {
            TabV17::Summary(v) => v.deadline(),
            TabV17::Conversation(v) | TabV17::Metadata(v) => v.deadline(),
        }
    }
    pub(crate) fn matches_capture(&self, app: &App) -> bool {
        match &self.0 {
            TabV17::Summary(v) => v.matches_capture(app),
            TabV17::Conversation(v) | TabV17::Metadata(v) => v.matches_capture(app),
        }
    }
    pub(crate) fn same_presentation(&self, app: &App) -> bool {
        match &self.0 {
            TabV17::Summary(v) => v.same_presentation(app),
            TabV17::Conversation(v) | TabV17::Metadata(v) => v.original.current_pointers(app),
        }
    }
    pub(crate) fn owns_presentation(&self, app: &App) -> bool {
        match &self.0 {
            TabV17::Summary(v) => v.owns_presentation(app),
            TabV17::Conversation(v) | TabV17::Metadata(v) => {
                self.same_presentation(app)
                    && app
                        .github
                        .as_ref()
                        .is_some_and(|github| github.scope().ok().as_ref() == Some(self.scope()))
                    && app.inbox_list.selected == v.original.selected_row
                    && app.inbox_list.top == v.original.scroll
            }
        }
    }
    pub(crate) fn retained_owns_v19(
        &self,
        app: &App,
        github: &crate::github_workspace::GithubWorkspace,
    ) -> bool {
        match &self.0 {
            TabV17::Summary(v) => v.retained_owns_v19(app, github),
            TabV17::Conversation(v) | TabV17::Metadata(v) => {
                v.original.retained_pointers_v19(app, github)
                    && github.scope().ok().as_ref() == Some(self.scope())
                    && app.inbox_list.selected == v.original.selected_row
                    && app.inbox_list.top == v.original.scroll
            }
        }
    }
    pub(crate) fn retire_account_mismatch(&self, app: &mut App) -> bool {
        if !self.same_presentation(app)
            || app
                .github
                .as_ref()
                .is_none_or(|github| github.scope().ok().as_ref() == Some(self.scope()))
        {
            return false;
        }
        // The exact original tab/page pointers above are mandatory. This is
        // independent of a replacement physical slot, with no slot/status/
        // loading/avatar-cache effect or stale private restoration.
        retire_original_presentation(app);
        true
    }
}
