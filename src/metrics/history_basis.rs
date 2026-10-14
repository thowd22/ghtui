//! Closed history interpretation, staged pending the coordinated model/store activation.
//! This is a pure projection label, never access authority or a producer proof constructor.
use serde::{Deserialize, Serialize};

pub const API_VISIBLE_HISTORY_LIMITATION: &str = "API-visible retained backfill only: erased/deleted events and non-atomic historical gaps are unknown; this is not a complete historical trend";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HistoryBasis {
    #[default]
    Unavailable,
    /// Only the sealed bounded producer plus validated actual SQL frame may supply this.
    /// Exhausted API-visible traversal is not proof that deleted history never existed.
    ApiVisibleRetainedBackfill { limitations: Vec<String> },
}
impl HistoryBasis {
    pub fn available_api_visible(&self) -> bool {
        matches!(self, Self::ApiVisibleRetainedBackfill {limitations}
            if limitations.len() <= 32 && limitations.iter().all(|reason| reason.len() <= 4096))
    }
    pub fn limitations(&self) -> Vec<String> {
        let Self::ApiVisibleRetainedBackfill { limitations } = self else {
            return vec![];
        };
        if !self.available_api_visible() {
            return vec![
                "Available-history source limitations exceeded their metadata budget".into(),
            ];
        }
        let mut limitations = limitations.clone();
        limitations.push(API_VISIBLE_HISTORY_LIMITATION.into());
        limitations.sort();
        limitations.dedup();
        limitations
    }
}
