//! Closed real Files-vs-Summary source boundary for the SAME Review queue.
//! Summary inspection cannot invent a Files record or publication authority.
use super::*;
use operations_v3::history_v3::ReviewOperationHistoryIntentV3;

#[derive(Clone, Copy)]
pub enum ReviewWorkOriginV3<'a> {
    Files(&'a Arc<ScopedReviewIntentV2>),
    Summary(&'a Arc<ReviewOperationHistoryIntentV3>),
}
impl<'a> ReviewWorkOriginV3<'a> {
    pub fn scope(self) -> &'a Scope {
        match self {
            Self::Files(intent) => intent.scope(),
            Self::Summary(intent) => intent.scope(),
        }
    }
    pub fn owns(self, app: &App) -> bool {
        match self {
            Self::Files(intent) => intent.owns(app),
            Self::Summary(intent) => intent.owns(app),
        }
    }
    pub fn source(self) -> &'a Arc<CatalogueIntentSource> {
        match self {
            Self::Files(intent) => &intent.source,
            Self::Summary(intent) => intent.source(),
        }
    }
    pub fn active(self) -> &'a Arc<ActiveRemoteScopeV1> {
        match self {
            Self::Files(intent) => &intent.active,
            Self::Summary(intent) => intent.active(),
        }
    }
    pub fn navigation(self) -> &'a Arc<ScopedPrNavigationV1> {
        match self {
            Self::Files(intent) => &intent.navigation,
            Self::Summary(intent) => intent.navigation(),
        }
    }
    pub fn files(self) -> Option<&'a Arc<ScopedReviewIntentV2>> {
        match self {
            Self::Files(intent) => Some(intent),
            Self::Summary(_) => None,
        }
    }
    pub fn summary(self) -> Option<&'a Arc<ReviewOperationHistoryIntentV3>> {
        match self {
            Self::Files(_) => None,
            Self::Summary(intent) => Some(intent),
        }
    }
}
