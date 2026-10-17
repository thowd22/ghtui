//! Bounded worker-only local body restoration. Original references remain
//! protected history, never Preview/approval or an automatic resend permit.
use crate::{
    forms::{Field, Form, PrFormKind, PrIntent},
    pr_actions::{PrCommand, SelectedComment},
    pr_collaboration_origin_v17::PrCollaborationOpenIntentV17,
    pr_write_journal::{PrPublicationReference, PrWriteJournal},
    providers::{
        gh::{
            CommentReaction, CurrentPrSubscription, GithubError, GithubErrorKind, GithubProvider,
            GithubResult,
        },
        runner::CancellationToken,
    },
    scoped_pr_collaboration_source_v17::{
        self as source, ConversationOpenObservationV17, PrCollaborationSourceV17,
    },
    storage::Cache,
};
use std::{collections::BTreeMap, path::Path, sync::Arc, time::Instant};

pub(crate) struct ConversationDraftIntentV17 {
    pub(crate) author: Option<ConversationActorV17>,
    pub(crate) source: Arc<PrCollaborationSourceV17>,
    pub(crate) command: PrCommand,
    pub(crate) selected: Option<SelectedComment>,
    pub(crate) reactions: Arc<[CommentReaction]>,
    pub(crate) subscription: Option<CurrentPrSubscription>,
    pub(crate) key: Arc<str>,
    pub(crate) original_lines: Arc<[String]>,
    pub(crate) action_base_v18: crate::pr_write_journal::conversation_action_lineage_v18::ConversationActionBaseV18,
    pub(crate) lineage_v18: Option<Arc<crate::pr_write_journal::conversation_action_lineage_v18::ConversationTerminalRevisionV18>>,
    pub(crate) needs_new_v18: bool,
    pub(crate) notice_painted_v18: std::sync::atomic::AtomicBool,
}
pub(crate) struct ConversationActorV17 {
    pub identity: crate::identity::ActorIdentity,
    pub observed: crate::providers::gh::Actor,
    pub label: Arc<str>,
}
pub(crate) fn prepare_actor_v17(
    host: &str,
    observed: &crate::providers::gh::Actor,
    uid: Option<u64>,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<ConversationActorV17> {
    check(cancel, deadline)?;
    crate::actor_avatar_projection_v1::actor_bytes(observed).map_err(|_| refusal())?;
    let identity = crate::identity::ActorIdentity::github(host, observed);
    let label = format!(
        "Original author {} • account {} (not requester)",
        identity.label(),
        crate::scoped_actions_workflow_ui_v6::account_caption_v25(uid)
    )
    .into();
    Ok(ConversationActorV17 {
        identity,
        observed: observed.clone(),
        label,
    })
}
pub(crate) struct OpenedConversationDraftV17 {
    pub(crate) intent: Arc<ConversationDraftIntentV17>,
    pub(crate) form: Form,
}
struct PreparedDraftV18 {
    form:Form,selected:Option<SelectedComment>,reactions:Vec<CommentReaction>,subscription:Option<CurrentPrSubscription>,
    key:Arc<str>,lines:Arc<[String]>,author:Option<ConversationActorV17>,
    base:crate::pr_write_journal::conversation_action_lineage_v18::ConversationActionBaseV18,
    lineage:Option<Arc<crate::pr_write_journal::conversation_action_lineage_v18::ConversationTerminalRevisionV18>>,
    needs_new:bool,
}
fn refusal() -> GithubError {
    GithubError {
        kind: GithubErrorKind::IdentityChanged,
        message: "Exact original Conversation fields/draft unavailable; no approval restored"
            .into(),
    }
}
fn check(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err(refusal())
    } else {
        Ok(())
    }
}
/// Exact envelope cap is checked through a counting writer BEFORE allocation.
/// This is the actual same body/reference format used by autosave + Confirm.
pub(crate) fn encode_draft_v17(
    values: &BTreeMap<String, String>,
    reference: Option<&PrPublicationReference>,
) -> Result<String, String> {
    if values.len() > 1 || values.values().any(|s| s.len() > 65_536) {
        return Err("Conversation fields exceed preclone bounds".into());
    }
    encode_envelope_v17(values, reference)
}
/// A closed data profile, not branch eligibility or publication authority.
/// Empty fields preserve an unsent draft; the genuine selected-member factory
/// separately proves the exact nonempty branch and full tip before Preview.
pub(crate) fn encode_base_draft_v32(
    values: &BTreeMap<String, String>,
    reference: Option<&PrPublicationReference>,
) -> Result<String, String> {
    if values.len() != 2
        || values.get("base").is_none_or(|s| s.len() > 1024)
        || values.get("tip").is_none_or(|s| s.len() > 64)
    {
        return Err("Exact bounded Base draft fields required".into());
    }
    encode_envelope_v17(values, reference)
}
fn encode_envelope_v17(
    values: &BTreeMap<String, String>,
    reference: Option<&PrPublicationReference>,
) -> Result<String, String> {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|n| *n <= 128 * 1024)
                .ok_or_else(|| std::io::Error::other("Encoded Conversation draft exceeds128KiB"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    #[derive(serde::Serialize)]
    struct Envelope<'a> {
        version: u8,
        values: &'a BTreeMap<String, String>,
        publication: Option<&'a PrPublicationReference>,
    }
    let envelope = Envelope {
        version: 17,
        values,
        publication: reference,
    };
    let mut count = Count(0);
    serde_json::to_writer(&mut count, &envelope).map_err(|e| e.to_string())?;
    let mut encoded = Vec::with_capacity(count.0);
    serde_json::to_writer(&mut encoded, &envelope).map_err(|e| e.to_string())?;
    String::from_utf8(encoded).map_err(|e| e.to_string())
}
impl OpenedConversationDraftV17 {
    pub(crate) fn open(
        opening: Arc<PrCollaborationOpenIntentV17>,
        command: PrCommand,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Self> {
        Self::open_phase_v18(opening, command, provider, database, cancel, deadline, None)
    }
    /// An explicit separate New action has a NEW queued token/absolute clock.
    /// It never reuses the notice's closed Core or reads its protected body.
    pub(crate) fn open_phase_v18(
        opening: Arc<PrCollaborationOpenIntentV17>,
        command: PrCommand,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
        expected:Option<&Arc<crate::pr_write_journal::conversation_action_lineage_v18::ConversationTerminalRevisionV18>>,
    ) -> GithubResult<Self> {
        let (opened, material) = source::open_with_lineage_preparation_v18(
            opening.clone(),
            command,
            expected.is_none(),
            provider,
            database,
            cancel,
            deadline,
            |parent, identity, observed, local, base| {
                check(cancel, deadline)?;
                let (selected, reactions, subscription) = match observed {
                    ConversationOpenObservationV17::New => (None, vec![], None),
                    ConversationOpenObservationV17::Comment(v) => {
                        v.require(command)?;
                        (Some(v.selected()), vec![], None)
                    }
                    ConversationOpenObservationV17::Reactions(v) => {
                        v.original().require(command)?;
                        if command == PrCommand::RemoveReaction && !v.complete() {
                            return Err(refusal());
                        }
                        let bytes = v
                            .reactions()
                            .iter()
                            .try_fold(0usize, |n, r| {
                                n.checked_add(r.content.len())?
                                    .checked_add(r.node_id.len())?
                                    .checked_add(r.actor.login.len())?
                                    .checked_add(128)
                            })
                            .filter(|n| *n <= 64 * 1024)
                            .ok_or_else(refusal)?;
                        let _ = bytes;
                        (Some(v.original().selected()), v.reactions().to_vec(), None)
                    }
                    ConversationOpenObservationV17::Subscription(v) => {
                        if !GithubProvider::same_thread_history_parent_v1(parent, v.parent()) {
                            return Err(refusal());
                        }
                        if !v.current().viewer_can_subscribe {
                            return Err(GithubError { kind: GithubErrorKind::Permission,
                                message: format!("Current subscription {} • viewerCanSubscribe=false; nothing sent", v.current().state_label()) });
                        }
                        (None, vec![], Some(v.current().clone()))
                    }
                };
                let author = match observed {
                    ConversationOpenObservationV17::Comment(v) => Some(prepare_actor_v17(
                        &identity.host,
                        &v.observed().author,
                        v.author_account(),
                        cancel,
                        deadline,
                    )?),
                    ConversationOpenObservationV17::Reactions(v) => Some(prepare_actor_v17(
                        &identity.host,
                        &v.original().observed().author,
                        v.original().author_account(),
                        cancel,
                        deadline,
                    )?),
                    _ => None,
                };
                let journal = PrWriteJournal::open(database).map_err(|_| refusal())?;
                if crate::pr_write_journal::conversation_history_v17::draft_key_v17(
                    opening.scope(),
                    parent,
                    command,
                    selected.as_ref(),
                )
                .map_err(|_| refusal())?
                    != base.key()
                {
                    return Err(refusal());
                }
                if let Some(expected) = expected {
                    journal
                        .validate_conversation_revision_v18(base, expected)
                        .map_err(|_| refusal())?;
                }
                let lineage = journal
                    .conversation_action_revision_v18(base)
                    .map_err(|_| refusal())?;
                if expected.is_some() && lineage.is_none() {
                    return Err(refusal());
                }
                let key = lineage.as_ref().map_or(base.key(), |r| r.key()).to_owned();
                let mut fields = crate::pr_actions::fields_v17(
                    command,
                    selected.as_ref(),
                    &reactions,
                    identity.account_id,
                    subscription.as_ref(),
                );
                let saved = Cache::open(database)
                    .map_err(|_| refusal())?
                    .bounded_draft(opening.scope(), &key, 128 * 1024)
                    .map_err(|_| refusal())?;
                let needs_new = lineage.is_some() && expected.is_none() && saved.is_none();
                // Actual current eligible choices are NOT an old request.
                // A new reaction intent must select content/owned ID again.
                if lineage.is_some()
                    && saved.is_none()
                    && matches!(command, PrCommand::AddReaction | PrCommand::RemoveReaction)
                {
                    for field in &mut fields {
                        field.value.clear();
                    }
                }
                let mut publication = None;
                if let Some(saved) = &saved {
                    #[derive(serde::Deserialize)]
                    #[serde(deny_unknown_fields)]
                    struct Envelope {
                        version: u8,
                        values: BTreeMap<String, String>,
                        publication: Option<PrPublicationReference>,
                    }
                    let envelope: Envelope = serde_json::from_str(saved).map_err(|_| refusal())?;
                    if envelope.version != 17
                        || envelope.values.len() != fields.len()
                        || envelope.values.values().any(|v| v.len() > 65_536)
                        || fields.iter().any(|f| !envelope.values.contains_key(f.key))
                    {
                        return Err(refusal());
                    }
                    for field in &mut fields {
                        field.value = envelope.values[field.key].clone();
                    }
                    publication = envelope.publication;
                    if let Some(reference) = &publication {
                        let journal = PrWriteJournal::open(database).map_err(|_| refusal())?;
                        let marker = journal
                            .original_publication(local.target_v17(), reference)
                            .map_err(|_| refusal())?
                            .ok_or_else(refusal)?;
                        let action = crate::pr_write_journal::conversation_history_v17::ConversationActionV17::from_command(command).map_err(|_| refusal())?;
                        if marker.command != action.command() {
                            return Err(refusal());
                        }
                    }
                }
                let mut form = Form::pr(PrIntent {
                    kind: PrFormKind::Comment,
                    target: parent.summary().clone(),
                    identity: identity.clone(),
                })
                .map_err(|_| refusal())?;
                form.title = format!(
                    "{} • {}#{}",
                    command.label(),
                    parent.repository(),
                    parent.number()
                );
                form.fields = fields
                    .into_iter()
                    .map(|value| {
                        let mut field =
                            Field::text(value.key, value.label, &value.value, value.multiline)?;
                        for choice in value.choices {
                            field.choices.push(choice.value);
                            field.choice_labels.push(choice.label);
                        }
                        Ok(field)
                    })
                    .collect::<Result<Vec<_>, String>>()
                    .map_err(|_| refusal())?;
                let requester = crate::identity::ActorIdentity::github(
                    &identity.host,
                    &crate::providers::gh::Actor {
                        login: identity.login.clone(),
                        actor_type: "User".into(),
                        ..Default::default()
                    },
                );
                let mut lines = vec![
                    format!(
                        "{} • fresh Preview + explicit Confirm required",
                        command.label()
                    ),
                    format!(
                        "{}#{} • actual repository #{} / PR #{}",
                        parent.repository(),
                        parent.number(),
                        parent.repository_id(),
                        parent.database_id()
                    ),
                    format!(
                        "Requester {} • account {}",
                        requester.label(),
                        crate::scoped_actions_workflow_ui_v6::account_caption_v25(
                            identity.account_id
                        )
                    ),
                    format!(
                        "Original H {} • base {}",
                        parent.head_sha(),
                        parent.base_tip_sha()
                    ),
                ];
                if command == PrCommand::NewComment {
                    lines.push("Create permission: UNKNOWN • GitHub decides when requested; fresh source is not permission".into());
                    lines.push("A denied request stays protected for inspection; never retried automatically".into());
                }
                if let ConversationOpenObservationV17::Reactions(v) = observed {
                    lines.push(format!(
                        "Current reactions: {} • {} actual pages • complete={}",
                        v.reactions().len(),
                        v.pages(),
                        v.complete()
                    ));
                }
                if let Some(comment) = match observed {
                    ConversationOpenObservationV17::Comment(v) => Some(v),
                    ConversationOpenObservationV17::Reactions(v) => Some(v.original()),
                    _ => None,
                } {
                    let capabilities = comment.capabilities();
                    let capability = |value| match value {
                        Some(true) => "reported allowed",
                        Some(false) => "reported not allowed",
                        None => "not reported",
                    };
                    lines.push(format!(
                        "Viewer capabilities • update: {} • delete: {} • react: {}",
                        capability(capabilities.can_update),
                        capability(capabilities.can_delete),
                        capability(capabilities.can_react)
                    ));
                    lines.push(format!(
                        "Viewer is author: {} • not login ownership or new write permission",
                        match capabilities.did_author {
                            Some(true) => "reported yes",
                            Some(false) => "reported no",
                            None => "not reported",
                        }
                    ));
                }
                if let Some(selected) = &selected {
                    let actor = crate::identity::ActorIdentity::github(
                        &identity.host,
                        &selected.observed.author,
                    );
                    lines.push(format!(
                        "Actual comment #{} • {} • original author {} (not necessarily requester)",
                        selected.observed.id,
                        selected.observed.node_id,
                        actor.label()
                    ));
                }
                if publication.is_some() {
                    lines.push("Original publication reference retained • inspect protected history; no resend or restored approval".into());
                }
                if let Some(revision) = &lineage {
                    lines.push(revision.label().into());
                    lines.push(format!(
                        "Exact terminal attempt #{} • old body/ref/origin remain inspect-only",
                        revision.reference().attempt_id
                    ));
                    if needs_new {
                        lines.push("NEEDS NEW ACTION • choose New action explicitly; a NEW current read starts before any editable draft. Nothing is replayed or resent.".into());
                        publication = Some(revision.reference().clone());
                    }
                }
                form.publication = publication;
                form.restored_draft = saved.is_some();
                // EVEN bodyless Delete starts a real normal autosave intent.
                // The publication fence must never treat absent revision as ACK.
                form.dirty = form.publication.is_none();
                if needs_new {
                    let display = crate::prepared_form_text_v1::PreparedFormTextV1::prepare(
                        &lines, cancel, deadline,
                    )
                    .map_err(|_| refusal())?;
                    form.preview = true;
                    form.focus = form.fields.len();
                    form.install_prepared_preview_display_v1(display);
                }
                // The producing source owns the exact Catalogue OR prepared
                // Personal reservation guard and rechecks it after this body
                // preparation and the real closing parent/repository. Never
                // invent a Files boundary just to run that validation here.
                check(cancel, deadline)?;
                Ok(PreparedDraftV18 {
                    form,
                    selected,
                    reactions,
                    subscription,
                    key: Arc::from(key),
                    lines: lines.into(),
                    author,
                    base: base.clone(),
                    lineage,
                    needs_new,
                })
            },
            |material| {
                if let Some(revision) = &material.lineage {
                    PrWriteJournal::open(database)
                        .map_err(|_| refusal())?
                        .validate_conversation_revision_v18(&material.base, revision)
                        .map_err(|_| refusal())?;
                }
                Ok(())
            },
        )?;
        let PreparedDraftV18 {
            form,
            selected,
            reactions,
            subscription,
            key,
            lines: original_lines,
            author,
            base: action_base_v18,
            lineage: lineage_v18,
            needs_new: needs_new_v18,
        } = material;
        Ok(Self {
            intent: Arc::new(ConversationDraftIntentV17 {
                source: opened.source,
                author,
                command,
                selected,
                reactions: reactions.into(),
                subscription,
                key,
                original_lines,
                action_base_v18,
                lineage_v18,
                needs_new_v18,
                notice_painted_v18: std::sync::atomic::AtomicBool::new(false),
            }),
            form,
        })
    }
}
