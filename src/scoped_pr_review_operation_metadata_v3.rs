//! Bounded original OBSERVATION for durable inspect-only recovery. This value
//! cannot construct source/approval/capabilities and is never deserialized into
//! a provider-issued original. Bodies remain solely in the existing draft.
use super::*;
use crate::pr_write_journal::PrWriteIntent;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResourceV3 {
    id: u64,
    node: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingMemberV3 {
    review: ResourceV3,
    comment: Option<ResourceV3>,
    path: Option<String>,
    commit: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewerV3 {
    kind: String,
    id: u64,
    node: String,
    review: Option<ResourceV3>,
    team_slug: Option<String>,
    organization: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OriginalReviewOperationMetadataV3 {
    version: u8,
    scope: Scope,
    repository_id: u64,
    repository_node: String,
    repository: String,
    pr_id: u64,
    pr_node: String,
    number: u64,
    head: String,
    base: String,
    command: String,
    draft_key: String,
    pending: Option<PendingMemberV3>,
    anchor: Option<ReviewAnchorMetadataV2>,
    reviewer: Option<ReviewerV3>,
}
impl OriginalReviewOperationMetadataV3 {
    pub(crate) fn from_source(source: &ScopedReviewOperationSourceV3) -> Result<Self, String> {
        let parent = source.intent.original.parent();
        // All source factories already bounded bodies; this pass bounds ONLY
        // borrowed metadata BEFORE creating any retained recovery allocation.
        let mut fields = vec![
            parent.repository(),
            parent.repository_node(),
            parent.node_id(),
            parent.head_sha(),
            parent.base_tip_sha(),
            source.key(),
        ];
        if let Some(review) = source.pending_review() {
            fields.push(review.snapshot().review().node());
        }
        if let Some(comment) = source.pending_comment() {
            fields.extend([
                comment.comment().node(),
                comment.comment().path(),
                comment.comment().commit(),
            ]);
        }
        if let Some(target) = source.rerequest().map(|source| source.target()) {
            fields.extend([
                target.node(),
                target.team_slug().unwrap_or(""),
                target.organization().unwrap_or(""),
            ]);
        }
        if fields.iter().any(|field| field.len() > 4096)
            || fields
                .iter()
                .try_fold(0usize, |sum, field| sum.checked_add(field.len()))
                .is_none_or(|sum| sum > 12 * 1024)
        {
            return Err(
                "Original review-operation observation exceeds borrowed16KiB preflight".into(),
            );
        }
        let pending = source.pending_review().map(|review| PendingMemberV3 {
            review: ResourceV3 {
                id: review.snapshot().review().id(),
                node: review.snapshot().review().node().into(),
            },
            comment: source.pending_comment().map(|comment| ResourceV3 {
                id: comment.comment().id(),
                node: comment.comment().node().into(),
            }),
            path: source
                .pending_comment()
                .map(|comment| comment.comment().path().into()),
            commit: source
                .pending_comment()
                .map(|comment| comment.comment().commit().into()),
        });
        let reviewer = source.rerequest().map(|actual| {
            let target = actual.target();
            ReviewerV3 {
                kind: match target.kind() {
                    crate::providers::gh::ReviewRequestActorKindV3::User => "User",
                    crate::providers::gh::ReviewRequestActorKindV3::Bot => "Bot",
                    crate::providers::gh::ReviewRequestActorKindV3::Team => "Team",
                }
                .into(),
                id: target.id(),
                node: target.node().into(),
                review: match &source.intent.choice {
                    ChoiceV3::Reviewer(selection) => Some(ResourceV3 {
                        id: selection.snapshot().review().id(),
                        node: selection.snapshot().review().node().into(),
                    }),
                    _ => None,
                },
                team_slug: target.team_slug().map(str::to_owned),
                organization: target.organization().map(str::to_owned),
            }
        });
        let anchor = match &source.intent.choice {
            ChoiceV3::Add { anchor, .. } => Some(anchor.as_ref().clone()),
            _ => None,
        };
        let value = Self {
            version: 3,
            scope: source.intent.original.scope().clone(),
            repository_id: parent.repository_id(),
            repository_node: parent.repository_node().into(),
            repository: parent.repository().into(),
            pr_id: parent.database_id(),
            pr_node: parent.node_id().into(),
            number: parent.number(),
            head: parent.head_sha().into(),
            base: parent.base_tip_sha().into(),
            command: source.intent.action().command().into(),
            draft_key: source.key().into(),
            pending,
            anchor,
            reviewer,
        };
        value.validate()?;
        value.encoded()?;
        Ok(value)
    }
    pub(crate) fn scope(&self) -> &Scope {
        &self.scope
    }
    pub(crate) fn repository_id(&self) -> u64 {
        self.repository_id
    }
    pub(crate) fn repository_node(&self) -> &str {
        &self.repository_node
    }
    pub(crate) fn repository(&self) -> &str {
        &self.repository
    }
    pub(crate) fn pr_id(&self) -> u64 {
        self.pr_id
    }
    pub(crate) fn pr_node(&self) -> &str {
        &self.pr_node
    }
    pub(crate) fn number(&self) -> u64 {
        self.number
    }
    pub(crate) fn head(&self) -> &str {
        &self.head
    }
    pub(crate) fn base(&self) -> &str {
        &self.base
    }
    pub(crate) fn command(&self) -> &str {
        &self.command
    }
    pub(crate) fn draft_key(&self) -> &str {
        &self.draft_key
    }
    pub(crate) fn history_lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!(
                "Historical original repo #{} {} • PR #{} {}",
                self.repository_id, self.repository_node, self.pr_id, self.pr_node
            ),
            format!(
                "Historical source H {} • base {} • NOT current source truth",
                self.head, self.base
            ),
        ];
        if let Some(pending) = &self.pending {
            lines.push(format!(
                "Recorded original server PENDING review #{} / {}",
                pending.review.id, pending.review.node
            ));
            if let Some(comment) = &pending.comment {
                lines.push(format!(
                    "Recorded original PENDING comment #{} / {} • path {} • commit {}",
                    comment.id,
                    comment.node,
                    pending.path.as_deref().unwrap_or("unavailable"),
                    pending.commit.as_deref().unwrap_or("unavailable")
                ));
            }
        }
        if let Some(reviewer) = &self.reviewer {
            lines.push(format!("Recorded actual reviewer {} #{} / {} • eligibility and notification delivery UNKNOWN",reviewer.kind,reviewer.id,reviewer.node));
        }
        if let Some(anchor) = &self.anchor {
            lines.push(format!("Historical requested canonical range {} {}–{} • server API filename {} • NOT current placement",
                if anchor.side()==DiffSide::Left{"LEFT"}else{"RIGHT"},anchor.first(),anchor.last(),anchor.server_path()));
        }
        lines
    }
    pub(crate) fn matches_reviewer_effect(
        &self,
        effect: &crate::pr_write_journal::PrConfirmedEffect,
    ) -> bool {
        self.reviewer.as_ref().is_some_and(|actual| {
            effect.kind == crate::pr_write_journal::PrEffectKind::Metadata
                && effect.resource_id == format!("reviewer-request/{}/{}", actual.kind, actual.id)
                && effect.node_id.as_deref() == Some(actual.node.as_str())
        })
    }
    pub(crate) fn matches_pending_effect(
        &self,
        effect: &crate::pr_write_journal::PrConfirmedEffect,
    ) -> bool {
        use crate::pr_write_journal::PrEffectKind;
        let Some(pending) = &self.pending else {
            return false;
        };
        match self.command.as_str() {
            "review-pending-comment-add-v3" => {
                effect.kind == PrEffectKind::Comment && pending.comment.is_none()
            }
            "review-pending-comment-edit-v3" | "review-pending-comment-delete-v3" => {
                pending.comment.as_ref().is_some_and(|comment| {
                    effect.kind
                        == (if self.command == "review-pending-comment-delete-v3" {
                            PrEffectKind::CommentDeleted
                        } else {
                            PrEffectKind::Comment
                        })
                        && effect.resource_id == comment.id.to_string()
                        && effect.node_id.as_deref() == Some(comment.node.as_str())
                })
            }
            _ => false,
        }
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        let positive = |id: u64| id > 0 && id <= i64::MAX as u64;
        let identifier = |text: &str| {
            !text.is_empty() && text.len() <= 1024 && !text.chars().any(char::is_control)
        };
        let resource = |value: &ResourceV3| positive(value.id) && identifier(&value.node);
        let digest = |text: &str| {
            text.len() == 64
                && text
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        let uid = self
            .scope
            .login
            .strip_prefix("user-id:")
            .and_then(|text| text.parse().ok())
            .filter(|id| positive(*id));
        if self.version != 3
            || Scope::github(&self.scope.host, uid).ok().as_ref() != Some(&self.scope)
            || !positive(self.repository_id)
            || !positive(self.pr_id)
            || !positive(self.number)
            || !identifier(&self.repository_node)
            || !identifier(&self.pr_node)
            || !crate::search::model::valid_sha(&self.head)
            || !crate::search::model::valid_sha(&self.base)
            || !self
                .draft_key
                .strip_prefix("review-operation-v3/")
                .is_some_and(digest)
        {
            return Err("Original review-operation recovery identity is malformed; no source permission restored".into());
        }
        crate::config::validate_repository(&self.repository)
            .map_err(|_| "Original repository path malformed")?;
        match(self.command.as_str(),&self.pending,&self.anchor,&self.reviewer){
            ("review-pending-comment-add-v3",Some(pending),Some(anchor),None)
                if resource(&pending.review)&&pending.comment.is_none()&&pending.path.is_none()&&pending.commit.is_none()=>{
                    anchor.validate()?;if anchor.head()!=self.head{return Err("Original add anchor H differs from recorded source".into());}
                },
            ("review-pending-comment-edit-v3"|"review-pending-comment-delete-v3",Some(pending),None,None)
                if resource(&pending.review)&&pending.comment.as_ref().is_some_and(resource)
                    &&pending.path.as_deref().is_some_and(|path|!path.is_empty()&&path.len()<=4096&&!path.contains('\0'))
                    &&pending.commit.as_deref().is_some_and(crate::search::model::valid_sha)=>{},
            ("review-rerequest-v3",None,None,Some(reviewer)) if positive(reviewer.id)&&identifier(&reviewer.node)=>{
                match reviewer.kind.as_str(){
                    "User"|"Bot" if reviewer.review.as_ref().is_some_and(resource)&&reviewer.team_slug.is_none()&&reviewer.organization.is_none()=>{},
                    "Team" if reviewer.review.is_none()&&reviewer.team_slug.as_deref().is_some_and(|slug|!slug.is_empty()&&slug.len()<=100
                        &&slug.bytes().all(|b|b.is_ascii_alphanumeric()||b"_-".contains(&b)))
                        &&reviewer.organization.as_deref().is_some_and(|organization|organization.len()<=100
                            &&self.repository.split_once('/').is_some_and(|(owner,_)|owner.eq_ignore_ascii_case(organization)))=>{},
                    _=>return Err("Original actual reviewer kind/source metadata is contradictory".into()),
                }
            },
            _=>return Err("Unknown or contradictory review-operation purpose/original target; no partial recovery".into()),
        }
        Ok(())
    }
    pub(crate) fn encoded(&self) -> Result<String, String> {
        self.validate()?;
        struct Count(usize);
        impl std::io::Write for Count {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0 = self
                    .0
                    .checked_add(bytes.len())
                    .filter(|n| *n <= 16 * 1024)
                    .ok_or_else(|| std::io::Error::other("Original observation exceeds16KiB"))?;
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut count = Count(0);
        serde_json::to_writer(&mut count, self)
            .map_err(|_| "Original observation exceeds16KiB before retained encoding")?;
        let mut bytes = Vec::with_capacity(count.0);
        serde_json::to_writer(&mut bytes, self)
            .map_err(|_| "Original observation encoding unavailable")?;
        String::from_utf8(bytes).map_err(|_| "Original observation encoding unavailable".into())
    }
    pub(crate) fn write(&self, body: &str) -> Result<PrWriteIntent, String> {
        if body.len() > 65_536
            || matches!(
                self.command.as_str(),
                "review-pending-comment-delete-v3" | "review-rerequest-v3"
            ) && !body.is_empty()
        {
            return Err("Original operation body differs from exact purpose".into());
        }
        let original = self.encoded()?;
        let selected = Sha256::digest(original.as_bytes());
        let prefix = if self.command == "review-rerequest-v3" {
            "review-request-v3"
        } else {
            "pending-comment-v3"
        };
        let write = PrWriteIntent {
            command: self.command.clone(),
            selected_resource: format!("{prefix}/{selected:x}"),
            observed_head: self.head.clone(),
            values: BTreeMap::from([
                ("body".into(), body.into()),
                ("original-observation".into(), original),
            ]),
        };
        write
            .digest()
            .map_err(|_| "Original operation fingerprint exceeds bounded payload")?;
        Ok(write)
    }
}
