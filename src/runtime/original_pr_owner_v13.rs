//! Closed genuine source variants. A Personal opening owns only its actual
//! retained App page; it cannot impersonate a Catalogue source or navigation.
use super::*;

pub(super) enum ScopedRemoteOriginalV13 {
    StateSourceV80(Arc<crate::state_inbox_sync_v76::StateSourceSyncV76>),
    CollaborationRefreshV19(
        Arc<crate::scoped_pr_collaboration_refresh_v19::return_v19::ConversationRefreshRequestV19>,
    ),
    CatalogueClearV5(Arc<super::catalogue_clear_v5::CatalogueClearOwnerV5>),
    CollaborationV17(Arc<crate::pr_collaboration_origin_v17::PrCollaborationOpenIntentV17>),
    Notifications(Arc<crate::notifications_v63::NotificationPresentationV63>),
    Catalogue {
        scope: Arc<crate::active_remote_scope_v1::ActiveRemoteScopeV1>,
        source: Arc<crate::storage::CatalogueIntentSource>,
        navigation: Option<Arc<crate::scoped_pr_observed_tabs_v1::ScopedPrNavigationV1>>,
    },
    Read {
        scope: Arc<crate::active_remote_scope_v1::ActiveRemoteScopeV1>,
        source: crate::pr_read_origin_v13::PrReadOriginV13,
        navigation: Arc<crate::scoped_pr_observed_tabs_v1::ScopedPrNavigationV1>,
    },
    OpeningPersonal(Arc<crate::personal_pr_entry_v13::PersonalPrEntryOwnerV13>),
    PersonalSummary(
        Arc<crate::personal_pr_entry_v13::summary_v14::PersonalPrSummaryPresentationV14>,
    ),
}
impl ScopedRemoteOriginalV13 {
    pub(super) fn catalogue(
        scope: Arc<crate::active_remote_scope_v1::ActiveRemoteScopeV1>,
        source: Arc<crate::storage::CatalogueIntentSource>,
        navigation: Option<Arc<crate::scoped_pr_observed_tabs_v1::ScopedPrNavigationV1>>,
    ) -> Self {
        Self::Catalogue {
            scope,
            source,
            navigation,
        }
    }

    pub(super) fn owns(&self, app: &App) -> bool {
        match self {
            Self::StateSourceV80(source) => source.matches_capture_v80(app) || source.owns(app),
            Self::CollaborationRefreshV19(request) => request.owns(app),
            Self::CatalogueClearV5(original) => original.owns(app),
            Self::CollaborationV17(original) => original.owns_presentation(app),
            Self::Notifications(original) => original.owns(app),
            Self::Catalogue {
                scope,
                source,
                navigation,
            } => {
                app.personal_pr_source_v13.is_none()
                    && app.notification_pr_source_v63.is_none()
                    && app
                        .remote_scope
                        .as_ref()
                        .is_some_and(|v| Arc::ptr_eq(v, scope))
                    && app
                        .remote_scope_source
                        .as_ref()
                        .is_some_and(|v| Arc::ptr_eq(v, source))
                    && navigation.as_ref().is_none_or(|n| {
                        app.github
                            .as_ref()
                            .and_then(|github| github.scoped_pr_navigation.as_ref())
                            .is_some_and(|v| Arc::ptr_eq(v, n))
                    })
            }
            Self::Read {
                scope,
                source,
                navigation,
            } => {
                source.matches_source(app)
                    && app
                        .remote_scope
                        .as_ref()
                        .is_some_and(|v| Arc::ptr_eq(v, scope))
                    && app.github.as_ref().is_some_and(|github| {
                        github.scope().ok().as_ref() == Some(source.scope())
                            && github
                                .scoped_pr_navigation
                                .as_ref()
                                .is_some_and(|v| Arc::ptr_eq(v, navigation))
                    })
            }
            Self::OpeningPersonal(owner) => owner.owns(app),
            Self::PersonalSummary(original) => original.owns_presentation(app),
        }
    }
    pub(super) fn navigation(
        &self,
    ) -> Option<&Arc<crate::scoped_pr_observed_tabs_v1::ScopedPrNavigationV1>> {
        match self {
            Self::StateSourceV80(_) => None,
            Self::CollaborationRefreshV19(_) => None,
            Self::CatalogueClearV5(_) => None,
            Self::CollaborationV17(original) => original.catalogue_navigation(),
            Self::Catalogue { navigation, .. } => navigation.as_ref(),
            Self::Read { navigation, .. } => Some(navigation),
            Self::OpeningPersonal(_) | Self::PersonalSummary(_) | Self::Notifications(_) => None,
        }
    }
    pub(super) fn scope(&self) -> &crate::storage::Scope {
        match self {
            Self::StateSourceV80(source) => source.original().scope(),
            Self::CollaborationRefreshV19(request) => request.original().scope(),
            Self::CatalogueClearV5(original) => original.scope(),
            Self::CollaborationV17(original) => original.scope(),
            Self::Notifications(original) => original.scope(),
            Self::Catalogue { source, .. } => source.scope(),
            Self::Read { source, .. } => source.scope(),
            Self::OpeningPersonal(owner) => owner.original_scope(),
            Self::PersonalSummary(original) => original.scope(),
        }
    }
    pub(super) fn opening_personal(
        &self,
    ) -> Option<&Arc<crate::personal_pr_entry_v13::PersonalPrEntryOwnerV13>> {
        match self {
            Self::OpeningPersonal(owner) => Some(owner),
            _ => None,
        }
    }
}
