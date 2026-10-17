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
    pub(crate) fn source(self) -> Option<crate::pr_read_origin_v13::PrReadOriginV13> {
        match self {
            Self::Files(intent) => Some(intent.source.clone()),
            Self::Summary(intent) => intent.catalogue_parts_v15().map(|(source, _, _)| {
                crate::pr_read_origin_v13::PrReadOriginV13::catalogue(source.clone())
            }),
        }
    }
    pub fn active(self) -> Option<&'a Arc<ActiveRemoteScopeV1>> {
        match self {
            Self::Files(intent) => Some(&intent.active),
            Self::Summary(intent) => intent.catalogue_parts_v15().map(|(_, active, _)| active),
        }
    }
    pub fn navigation(self) -> Option<&'a Arc<ScopedPrNavigationV1>> {
        match self {
            Self::Files(intent) => Some(&intent.navigation),
            Self::Summary(intent) => intent
                .catalogue_parts_v15()
                .map(|(_, _, navigation)| navigation),
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
