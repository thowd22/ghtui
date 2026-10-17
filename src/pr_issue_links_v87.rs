//! The public action vocabulary is deliberately payload-free. Issue identities,
//! choices, observations and plans remain inside the original gh producer.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueLinkCommandV87 {
    LinkIssue,
    UnlinkManualIssue,
}

impl IssueLinkCommandV87 {
    pub fn label(self) -> &'static str {
        match self {
            Self::LinkIssue => "Link Issue",
            Self::UnlinkManualIssue => "Unlink manual Issue",
        }
    }

    pub(crate) fn command(self) -> &'static str {
        match self {
            Self::LinkIssue => "issue-link-v87",
            Self::UnlinkManualIssue => "issue-unlink-manual-v87",
        }
    }

    pub(crate) fn mutation(self) -> &'static str {
        match self {
            Self::LinkIssue => "addCloseIssueReferences",
            Self::UnlinkManualIssue => "removeCloseIssueReferences",
        }
    }
}
