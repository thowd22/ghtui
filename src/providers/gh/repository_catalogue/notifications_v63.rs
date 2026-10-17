//! Genuine notification observations. A REST row, URL or reason is NOT current
//! repository access, an acting user's identity, a PR source or write authority.
use super::*;

#[path = "notifications_v63/conditional.rs"]
mod conditional;
#[path = "notifications_v63/context.rs"]
mod context;
#[path = "notifications_v63/original.rs"]
mod original;
#[path = "notifications_v63/write_origin.rs"]
mod write_origin;
pub(crate) use context::NotificationReadContextV63;
pub(super) use model::{
    NotificationAccumulatorV63, NotificationPageV63, NotificationRepositoryV63,
    NotificationSubjectV63,
};
pub(super) use original::NotificationRepositoryOriginalV63;
pub(crate) use write_origin::NotificationWriteOriginV63;
#[path = "notifications_v63/model.rs"]
mod model;
#[path = "notifications_v63/parse.rs"]
pub(super) mod parse;
#[path = "notifications_v63/poll.rs"]
pub(super) mod poll;
pub(crate) use conditional::NotificationConditionalRequestV63;
pub(crate) use model::{NotificationQueryV63, NotificationRowV63};
pub(crate) use poll::NotificationWatchV63;

const PAGE_CAP_V63: usize = 256 * 1024;
const METADATA_CAP_V63: usize = 4 * 1024 * 1024;
const MAX_ROWS_V63: usize = 4000;

fn bad_v63(kind: GithubErrorKind) -> GithubError {
    error(kind, "Notification observation is incomplete, conflicting or outside its bounded original source")
}

#[cfg(test)]
#[path = "notifications_v63/tests.rs"]
mod tests;
