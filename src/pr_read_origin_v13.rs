//! Closed selected-PR origin; a source is necessary, never sufficient to write.
use crate::{
    active_remote_scope_v1::{ActiveRemoteScopeV1, RemoteScopeConsumerV1},
    app::App,
    github_workspace::PrKey,
    providers::{
        gh::{
            GithubProvider, HostIdentity, OriginalPrAccessSessionV13, OriginalPrMembershipRefV13,
            RepositoryCatalogueAccessLimits,
        },
        runner::CancellationToken,
    },
    scoped_pr_observed_tabs_v1::ScopedPrNavigationV1,
    storage::{Cache, CatalogueIntentSource, PersonalPrSourceV13, Scope},
};
use std::sync::{Arc, Weak};
#[derive(Clone)]
enum OriginKindV13 {
    Catalogue(Arc<CatalogueIntentSource>),
    Personal(Arc<PersonalPrSourceV13>),
    Notifications(Arc<crate::providers::gh::NotificationPrSourceV63>),
}
#[derive(Clone)]
pub(crate) struct PrReadOriginV13(OriginKindV13);
/// Display-only weak allocation marker; it cannot retain PR bodies or issue a
/// source, pool, account proof or publication intent.
pub(crate) enum WeakPrReadOriginV14 {
    Catalogue(Weak<CatalogueIntentSource>),
    Personal(Weak<PersonalPrSourceV13>),
    Notifications(Weak<crate::providers::gh::NotificationPrSourceV63>),
}
impl WeakPrReadOriginV14 {
    pub(crate) fn matches_source(&self, app: &App) -> bool {
        match self {
            Self::Catalogue(source) => source
                .upgrade()
                .is_some_and(|source| PrReadOriginV13::catalogue(source).matches_source(app)),
            Self::Personal(source) => source.upgrade().is_some_and(|source| {
                PrReadOriginV13(OriginKindV13::Personal(source)).matches_source(app)
            }),
            Self::Notifications(source) => source.upgrade().is_some_and(|source| {
                PrReadOriginV13(OriginKindV13::Notifications(source)).matches_source(app)
            }),
        }
    }
}
impl PrReadOriginV13 {
    pub(crate) fn downgrade_v14(&self) -> WeakPrReadOriginV14 {
        match &self.0 {
            OriginKindV13::Catalogue(source) => {
                WeakPrReadOriginV14::Catalogue(Arc::downgrade(source))
            }
            OriginKindV13::Personal(source) => {
                WeakPrReadOriginV14::Personal(Arc::downgrade(source))
            }
            OriginKindV13::Notifications(source) => {
                WeakPrReadOriginV14::Notifications(Arc::downgrade(source))
            }
        }
    }
    /// Exact current original account loss, independent of an old physical
    /// queue fence. An independently newer source/navigation is never erased.
    pub(crate) fn retire_account_mismatch_v14(
        &self,
        app: &mut App,
        active: &Arc<ActiveRemoteScopeV1>,
        navigation: &Arc<ScopedPrNavigationV1>,
    ) -> bool {
        if !self.matches_source(app)
            || app
                .remote_scope
                .as_ref()
                .is_none_or(|current| !Arc::ptr_eq(current, active))
            || app
                .github
                .as_ref()
                .and_then(|g| g.scoped_pr_navigation.as_ref())
                .is_none_or(|current| !Arc::ptr_eq(current, navigation))
            || app
                .github
                .as_ref()
                .is_none_or(|g| g.scope().ok().as_ref() == Some(self.scope()))
        {
            return false;
        }
        match &self.0 {
            OriginKindV13::Catalogue(source) => {
                crate::remote_scope_gate_v1::retire_original_account_mismatch(app, source, active)
            }
            OriginKindV13::Personal(_) => {
                crate::personal_pr_entry_v13::retire_source(app, self, navigation)
            }
            OriginKindV13::Notifications(_) => {
                crate::notification_pr_v63::retire_account_mismatch(app, self, active, navigation)
            }
        }
    }
    pub(crate) fn catalogue(source: Arc<CatalogueIntentSource>) -> Self {
        Self(OriginKindV13::Catalogue(source))
    }
    pub(crate) fn capture(app: &App) -> Result<Self, String> {
        if crate::scoped_pr_collaboration_native_v17::retired_presentation(app) {
            return Err("Hidden original Conversation page cannot issue new read intent".into());
        }
        let active = app
            .remote_scope
            .as_ref()
            .ok_or("Original PR scope unavailable")?;
        let github = app.github.as_ref().ok_or("Original PR page unavailable")?;
        match active.consumer(&github.identity) {
            RemoteScopeConsumerV1::Explicit(_)
                if app.personal_pr_source_v13.is_none()
                    && app.notification_pr_source_v63.is_none() =>
            {
                app.remote_scope_source
                    .clone()
                    .map(Self::catalogue)
                    .ok_or_else(|| "Original catalogue source unavailable".into())
            }
            RemoteScopeConsumerV1::DefaultPersonal => app
                .personal_pr_source_v13
                .as_ref()
                .filter(|state| state.owns(app))
                .map(|state| Self(OriginKindV13::Personal(state.source().clone())))
                .ok_or_else(|| {
                    "Genuine original personal source unavailable; load Files explicitly".into()
                }),
            RemoteScopeConsumerV1::NotificationPr(member) => app
                .notification_pr_source_v63
                .as_ref()
                .filter(|state| state.owns(app) && Arc::ptr_eq(state.source(), member.source()))
                .map(|state| Self(OriginKindV13::Notifications(state.source().clone())))
                .ok_or_else(|| {
                    "Actual notification PR source/page unavailable; no source fallback".into()
                }),
            _ => Err("Original scope cannot fall back to another source kind".into()),
        }
    }
    pub(crate) fn scope(&self) -> &Scope {
        match &self.0 {
            OriginKindV13::Catalogue(v) => v.scope(),
            OriginKindV13::Personal(v) => v.scope(),
            OriginKindV13::Notifications(v) => v.scope(),
        }
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        match &self.0 {
            OriginKindV13::Catalogue(v) => v.source().identity(),
            OriginKindV13::Personal(v) => v.identity(),
            OriginKindV13::Notifications(v) => v.identity(),
        }
    }
    pub(crate) fn same_allocation(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (OriginKindV13::Catalogue(a), OriginKindV13::Catalogue(b)) => Arc::ptr_eq(a, b),
            (OriginKindV13::Personal(a), OriginKindV13::Personal(b)) => Arc::ptr_eq(a, b),
            (OriginKindV13::Notifications(a), OriginKindV13::Notifications(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
    /// Pointer-only origin identity; numeric-current-account checks remain
    /// separate so genuine account loss can hide the original private surface.
    pub(crate) fn matches_source(&self, app: &App) -> bool {
        match &self.0 {
            OriginKindV13::Catalogue(source) => {
                app.personal_pr_source_v13.is_none()
                    && app.notification_pr_source_v63.is_none()
                    && app
                        .remote_scope_source
                        .as_ref()
                        .is_some_and(|v| Arc::ptr_eq(v, source))
            }
            OriginKindV13::Personal(source) => app
                .personal_pr_source_v13
                .as_ref()
                .is_some_and(|state| Arc::ptr_eq(state.source(), source) && state.same_page(app)),
            OriginKindV13::Notifications(source) => app
                .notification_pr_source_v63
                .as_ref()
                .is_some_and(|state| Arc::ptr_eq(state.source(), source) && state.same_page(app)),
        }
    }
    pub(crate) fn is_personal(&self) -> bool {
        matches!(&self.0, OriginKindV13::Personal(_))
    }
    pub(crate) fn is_notification_v63(&self) -> bool {
        matches!(&self.0, OriginKindV13::Notifications(_))
    }
    pub(crate) fn requires_original_bounds_v63(&self) -> bool {
        matches!(
            &self.0,
            OriginKindV13::Personal(_) | OriginKindV13::Notifications(_)
        )
    }
    /// Explicit purpose-specific boundary, NEVER a fallback between variants.
    /// Until C consumes this common origin, old Catalogue-only writers must
    /// refuse the genuine Personal case rather than invent a Catalogue source.
    pub(crate) fn catalogue_source(&self) -> Option<&Arc<CatalogueIntentSource>> {
        match &self.0 {
            OriginKindV13::Catalogue(source) => Some(source),
            OriginKindV13::Personal(_) => None,
            OriginKindV13::Notifications(_) => None,
        }
    }
    pub(crate) fn local_boundary(
        &self,
    ) -> crate::scoped_pr_observed_tabs_v1::resources::PrLocalSourceBoundaryV13<'_> {
        use crate::scoped_pr_observed_tabs_v1::resources::PrLocalSourceBoundaryV13 as B;
        match &self.0 {
            OriginKindV13::Catalogue(source) => B::Catalogue(source),
            OriginKindV13::Personal(source) => B::Personal(source),
            OriginKindV13::Notifications(source) => B::Notifications(source),
        }
    }
    pub(crate) fn same_catalogue_source(&self, other: &Arc<CatalogueIntentSource>) -> bool {
        self.catalogue_source()
            .is_some_and(|source| Arc::ptr_eq(source, other))
    }
    pub(crate) fn validate_cache(&self, cache: &Cache) -> Result<(), String> {
        match &self.0 {
            OriginKindV13::Catalogue(v) => cache
                .validate_repository_catalogue_intent_source(v)
                .map_err(|e| e.to_string()),
            OriginKindV13::Personal(v) => cache
                .validate_personal_pr_source_v13(v)
                .map_err(|e| e.to_string()),
            OriginKindV13::Notifications(v) => v.validate_cache(cache).map_err(|e| e.to_string()),
        }
    }
    pub(crate) fn begin_access<'a>(
        &'a self,
        provider: &'a GithubProvider,
        limits: &RepositoryCatalogueAccessLimits,
        cancel: &CancellationToken,
    ) -> crate::providers::gh::GithubResult<OriginalPrAccessSessionV13<'a>> {
        let original = match &self.0 {
            OriginKindV13::Catalogue(v) => OriginalPrMembershipRefV13::Catalogue(v.source()),
            OriginKindV13::Personal(v) => OriginalPrMembershipRefV13::Personal(v.original()),
            OriginKindV13::Notifications(v) => OriginalPrMembershipRefV13::Notifications(v),
        };
        provider.begin_original_pr_access_v13(original, limits, cancel)
    }
    /// Existing Review membership semantics: exact genuine original reference,
    /// not a cached last-name authorization. The worker still verifies the real
    /// numeric/node repository and actual raw parent on the SAME Core.
    pub(crate) fn admits_review_navigation_v14(
        &self,
        active: &ActiveRemoteScopeV1,
        identity: &HostIdentity,
        navigation: &ScopedPrNavigationV1,
    ) -> bool {
        if navigation.scope() != self.scope()
            || Scope::github(&identity.host, identity.account_id)
                .ok()
                .as_ref()
                != Some(self.scope())
        {
            return false;
        }
        match (&self.0, active.consumer(identity)) {
            (OriginKindV13::Catalogue(_), RemoteScopeConsumerV1::Explicit(rows)) => rows
                .references()
                .iter()
                .any(|row| row.original() == navigation.original()),
            (OriginKindV13::Personal(source), RemoteScopeConsumerV1::DefaultPersonal) => {
                source.reference() == navigation.original()
            }
            (
                OriginKindV13::Notifications(source),
                RemoteScopeConsumerV1::NotificationPr(member),
            ) => {
                Arc::ptr_eq(source, member.source())
                    && source.reference() == navigation.original()
                    && navigation.matches_parent_v63(source.read().parent())
            }
            _ => false,
        }
    }
    pub(crate) fn admits_navigation(
        &self,
        active: &ActiveRemoteScopeV1,
        identity: &HostIdentity,
        nav: &ScopedPrNavigationV1,
        selected: &PrKey,
    ) -> bool {
        if nav.scope() != self.scope()
            || Scope::github(&identity.host, identity.account_id)
                .ok()
                .as_ref()
                != Some(self.scope())
        {
            return false;
        }
        match (&self.0, active.consumer(identity)) {
            (OriginKindV13::Catalogue(_), RemoteScopeConsumerV1::Explicit(rows)) => {
                rows.references().iter().any(|v| {
                    v.original() == nav.original()
                        && v.observed()
                            .is_some_and(|r| r.last_name.eq_ignore_ascii_case(&selected.repository))
                })
            }
            (OriginKindV13::Personal(v), RemoteScopeConsumerV1::DefaultPersonal) => {
                v.reference() == nav.original()
                    && selected.number == v.read().parent().number()
                    && selected.host == v.scope().host
            }
            (OriginKindV13::Notifications(v), RemoteScopeConsumerV1::NotificationPr(member)) => {
                Arc::ptr_eq(v, member.source())
                    && v.reference() == nav.original()
                    && selected.number == v.read().parent().number()
                    && selected.host == v.scope().host
                    && selected
                        .repository
                        .eq_ignore_ascii_case(v.read().parent().repository())
            }
            _ => false,
        }
    }
}
