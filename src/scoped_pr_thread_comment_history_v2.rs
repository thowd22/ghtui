//! Separate fresh-Summary inspector for exact protected comment/reaction
//! origins. No ResourceLocalGuard weakening, current-comment lookup or write.
use super::*;
use crate::pr_write_journal::thread_comment_history_v2::CommentHistorySnapshotV2;
#[path = "scoped_pr_thread_comment_history_ui_v2.rs"]
pub mod ui;
pub struct PreparedCommentHistoryV2 {
    pub intent: Arc<ThreadHistoryIntentV1>,
    pub identity: HostIdentity,
    pub header: Arc<[String]>,
    pub labels: Arc<[String]>,
    pub roles: Arc<[SemanticToken]>,
    pub actor: ActorIdentity,
    pub opened: Option<usize>,
    pub detail: Arc<[String]>,
    snapshot: CommentHistorySnapshotV2,
}
pub fn load(
    intent: Arc<ThreadHistoryIntentV1>,
    previous: Option<(&Arc<PreparedCommentHistoryV2>, usize)>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<Arc<PreparedCommentHistoryV2>, ThreadBrowseFailureV1> {
    original_check(cancel, deadline).map_err(source_error)?;
    if previous.is_some_and(|(old, index)| {
        !Arc::ptr_eq(&old.intent, &intent) || index >= old.snapshot.rows.len()
    }) {
        return Err(ThreadBrowseFailureV1::Source);
    }
    let provider = provider.clone().with_catalogue_read_deadline(deadline);
    let phase =
        phase_v75::HistoryReadPhaseV75::begin(&intent, &provider, database, cancel, deadline)?;
    let identity = phase.identity().clone();
    let parent = phase.parent().clone();
    original_check(cancel, deadline).map_err(source_error)?;
    let journal = PrWriteJournal::open(database).map_err(source_error)?;
    let target = PrWriteTarget::new(&identity, parent.summary()).map_err(source_error)?;
    if let Some((old, _)) = previous {
        journal
            .validate_thread_comment_history_v2(
                &target,
                parent.repository_id(),
                parent.repository_node(),
                parent.database_id(),
                &old.snapshot,
            )
            .map_err(source_error)?;
    }
    let snapshot = journal
        .thread_comment_history_v2(
            &target,
            parent.repository_id(),
            parent.repository_node(),
            parent.database_id(),
        )
        .map_err(source_error)?;
    let opened = previous.map(|(_, index)| index);
    let mut detail = Vec::new();
    let mut body_witness = None;
    let mut recorded_actor = None;
    if let Some(index) = opened {
        let row = snapshot
            .rows
            .get(index)
            .ok_or(ThreadBrowseFailureV1::Source)?;
        let marker = &row.marker;
        detail.extend([format!("Recorded attempt #{} • {:?} • {}",marker.attempt_id,marker.state,marker.command),
            format!("Local journal UNIX {} seconds; NOT a server event timestamp",marker.created_at),
            format!("Historical PR H {} (NOT current H)",marker.observed_head),
            format!("Exact durable payload fingerprint {}",marker.payload_sha256),
            "INSPECT ONLY. No restore, retry, reconciliation, discard, acknowledgement or approval. Current comment/reaction existence and placement are UNKNOWN, including after deletion.".into()]);
        if let Some(origin) = &row.origin {
            let seed = &origin.seed;
            detail.extend([format!("Recorded action account {} • observed login {} (not proof of authorship/causality)",seed.scope.login,seed.observed_login),
                format!("Historical repo #{} {} • PR #{} {}",seed.repository_id,seed.repository_node,seed.pr_id,seed.pr_node),
                format!("Historical base {} • actual opaque thread {}",seed.base,seed.thread_node),
                format!("Historical actual comment #{} / {} • recorded numeric author {:?}",seed.comment_id,seed.comment_node,seed.author_account_id),
                format!("Observed historical comment C {} • original C {} • raw observed path {}",seed.comment_commit,seed.original_commit.as_deref().unwrap_or("unavailable"),seed.observed_path),
                format!("Observed source Updated {} • original body SHA256 {} (not a second body)",seed.observed_updated,seed.observed_body_sha256)]);
            let actor = seed
                .observed_author
                .as_ref()
                .map(|actor| crate::providers::gh::Actor {
                    login: actor.login.clone(),
                    actor_type: actor.actor_type.clone(),
                    display_name: actor.display_name.clone(),
                    avatar_url: actor.avatar_url.clone(),
                })
                .unwrap_or_default();
            let actor = ActorIdentity::github(&identity.host, &actor);
            detail.push(format!(
                "Recorded comment author {} • current inspector is independent",
                actor.label()
            ));
            recorded_actor = Some(actor);
            if let Some(effect) = &marker.confirmed {
                detail.push(format!(
                    "Recorded observed receipt {:?} #{} • node {} (not proof of causality)",
                    effect.kind,
                    effect.resource_id,
                    effect.node_id.as_deref().unwrap_or("unavailable")
                ));
            }
            original_check(cancel, deadline).map_err(source_error)?;
            let key = seed.draft_key().map_err(source_error)?;
            let encoded = Cache::open(database)
                .map_err(source_error)?
                .bounded_draft(intent.scope(), &key, 128 * 1024)
                .map_err(source_error)?;
            if let Some(encoded) = &encoded {
                let value: serde_json::Value =
                    serde_json::from_str(encoded).map_err(source_error)?;
                let object = value
                    .as_object()
                    .filter(|object| object.len() == 3)
                    .ok_or(ThreadBrowseFailureV1::Source)?;
                let values = object
                    .get("values")
                    .and_then(Value::as_object)
                    .filter(|values| values.len() == 1)
                    .ok_or(ThreadBrowseFailureV1::Source)?;
                let body = values
                    .get("body")
                    .and_then(Value::as_str)
                    .filter(|body| body.len() <= 65_536)
                    .ok_or(ThreadBrowseFailureV1::Source)?;
                let reference = object
                    .get("publication")
                    .and_then(Value::as_object)
                    .ok_or(ThreadBrowseFailureV1::Source)?;
                if reference.len() != 4
                    || ![
                        "attempt_id",
                        "pr_node_id",
                        "observed_head",
                        "payload_sha256",
                    ]
                    .iter()
                    .all(|field| reference.contains_key(*field))
                {
                    return Err(ThreadBrowseFailureV1::Source);
                }
                let reference: crate::pr_write_journal::PrPublicationReference =
                    serde_json::from_value(value["publication"].clone()).map_err(source_error)?;
                if object.get("version").and_then(Value::as_u64) != Some(2)
                    || reference != origin.reference
                    || seed
                        .write(body)
                        .and_then(|write| write.digest())
                        .map_err(source_error)?
                        != marker.payload_sha256
                {
                    return Err(ThreadBrowseFailureV1::Source);
                }
                detail.push("Exact protected LOCAL requested body (not a current server comment or new draft):".into());
                detail.extend(body.split('\n').map(str::to_owned));
            } else {
                detail.push(if marker.state==PrWriteState::Confirmed {"Local requested body unavailable; may have been removed after confirmation. Exact recorded receipt remains inspectable."}else{"Local requested body unavailable; ACK/persistence is unknown, NOT an empty body or resend permission."}.into());
            }
            body_witness = Some((key, encoded));
        } else {
            detail.push("Protected legacy comment/reaction record: exact origin unavailable; body is NOT opened and no opaque target, current existence, placement or approval is guessed.".into());
        }
    }
    let labels = snapshot
        .rows
        .iter()
        .map(|row| {
            escape(&format!(
                "#{} {:?} {} • {}",
                row.marker.attempt_id,
                row.marker.state,
                row.marker.command,
                if row.origin.is_some() {
                    "exact V2 origin recorded"
                } else {
                    "protected legacy • target unavailable"
                }
            ))
        })
        .collect::<Vec<_>>();
    let roles = snapshot
        .rows
        .iter()
        .map(|row| match row.marker.state {
            PrWriteState::Pending => SemanticToken::Pending,
            PrWriteState::Uncertain => SemanticToken::Conflict,
            PrWriteState::Confirmed => SemanticToken::Success,
            PrWriteState::Acknowledged => SemanticToken::Muted,
        })
        .collect::<Vec<_>>();
    let header=vec![escape(&format!("Fresh current {}#{} • repo #{} / PR #{}",parent.repository(),parent.number(),parent.repository_id(),parent.database_id())),
        escape(&format!("Current H {} • current base {}",parent.head_sha(),parent.base_tip_sha())),
        escape(&format!("Latest {} selected-comment/reaction attempts • {} • current numeric inspector {:?}",snapshot.rows.len(),snapshot.total.map_or_else(||"further rows unknown".into(),|count|format!("complete bounded enumeration {count}")),identity.account_id)),
        "Inspect only • historical targets/actors differ from current source; no write controls or current-placement inference".into()];
    let detail = detail
        .into_iter()
        .map(|line| escape(&line))
        .collect::<Vec<_>>();
    if header
        .iter()
        .chain(&labels)
        .chain(&detail)
        .map(String::len)
        .sum::<usize>()
        > 1024 * 1024
        || detail.len() > 65_536
    {
        return Err(ThreadBrowseFailureV1::Display);
    }
    let identity = phase.finish(|| {
        original_check(cancel, deadline)?;
        let cache = Cache::open(database).map_err(|_| safe_local())?;
        PrWriteJournal::open(database)
            .map_err(|_| safe_local())?
            .validate_thread_comment_history_v2(
                &target,
                parent.repository_id(),
                parent.repository_node(),
                parent.database_id(),
                &snapshot,
            )
            .map_err(|_| safe_local())?;
        if let Some((key, expected)) = &body_witness {
            if cache
                .bounded_draft(intent.scope(), key, 128 * 1024)
                .map_err(|_| safe_local())?
                .as_ref()
                != expected.as_ref()
            {
                return Err(safe_local());
            }
        }
        original_check(cancel, deadline)
    })?;
    let actor = recorded_actor.unwrap_or_else(|| {
        ActorIdentity::github(
            &identity.host,
            &crate::providers::gh::Actor {
                login: if opened.is_none() {
                    identity.login.clone()
                } else {
                    String::new()
                },
                actor_type: "User".into(),
                ..Default::default()
            },
        )
    });
    Ok(Arc::new(PreparedCommentHistoryV2 {
        intent,
        identity,
        header: header.into(),
        labels: labels.into(),
        roles: roles.into(),
        actor,
        opened,
        detail: detail.into(),
        snapshot,
    }))
}
