//! Fresh Summary-only protected FILE attempts. No current-member inference,
//! restore, approval, retry, reconciliation, acknowledgement or deletion.
use super::*;
use crate::pr_write_journal::file_comment_history_v75::FileCommentHistorySnapshotV75;
#[path = "scoped_pr_file_comment_history_ui_v75.rs"]
pub mod ui;
pub struct PreparedFileCommentHistoryV75 {
    pub intent: Arc<ThreadHistoryIntentV1>,
    pub identity: HostIdentity,
    pub header: Arc<[String]>,
    pub labels: Arc<[String]>,
    pub roles: Arc<[SemanticToken]>,
    pub actor: ActorIdentity,
    pub opened: Option<usize>,
    pub detail: Arc<[String]>,
    snapshot: FileCommentHistorySnapshotV75,
}
pub fn load(
    intent: Arc<ThreadHistoryIntentV1>,
    previous: Option<(&Arc<PreparedFileCommentHistoryV75>, usize)>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<Arc<PreparedFileCommentHistoryV75>, ThreadBrowseFailureV1> {
    original_check(cancel, deadline).map_err(source_error)?;
    if previous.is_some_and(|(old, index)| {
        !Arc::ptr_eq(&old.intent, &intent) || index >= old.snapshot.rows.len()
    }) {
        return Err(ThreadBrowseFailureV1::Source);
    }
    let provider = provider.clone().with_catalogue_read_deadline(deadline);
    let phase =
        phase_v75::HistoryReadPhaseV75::begin(&intent, &provider, database, cancel, deadline)?;
    let parent = phase.parent().clone();
    let identity = phase.identity().clone();
    let target = PrWriteTarget::new(&identity, parent.summary()).map_err(source_error)?;
    let journal = PrWriteJournal::open(database).map_err(source_error)?;
    if let Some((old, _)) = previous {
        journal
            .validate_file_comment_history_v75(
                &target,
                parent.repository_id(),
                parent.repository_node(),
                parent.database_id(),
                &old.snapshot,
            )
            .map_err(source_error)?;
    }
    let snapshot = journal
        .file_comment_history_v75(
            &target,
            parent.repository_id(),
            parent.repository_node(),
            parent.database_id(),
        )
        .map_err(source_error)?;
    let opened = previous.map(|(_, index)| index);
    let mut detail = Vec::new();
    let mut body_witness = None;
    let mut observed_login = None;
    if let Some(index) = opened {
        let row = snapshot
            .rows
            .get(index)
            .ok_or(ThreadBrowseFailureV1::Source)?;
        let marker = &row.marker;
        detail.extend([
            format!("Recorded FILE attempt #{} • {:?} • {}", marker.attempt_id, marker.state, marker.command),
            format!("Local journal UNIX {} seconds; NOT a server timestamp", marker.created_at),
            format!("Historical H {} • exact durable fingerprint {}", marker.observed_head, marker.payload_sha256),
            "INSPECT ONLY. No line/range/thread/current placement, restore, retry, discard, acknowledgement or publication permission is inferred.".into(),
        ]);
        if let Some(origin) = &row.origin {
            let seed = &origin.seed;
            detail.extend([
                format!("Original numeric actor {} • observed username {} (not current inspector/causality)", seed.scope.login, seed.observed_login),
                format!("Historical repo #{} {} • PR #{} {}", seed.repository_id, seed.repository_node, seed.pr_id, seed.pr_node),
                format!("Historical FILE raw path {} • previous path {}", seed.path, seed.previous_path.as_deref().unwrap_or("unavailable")),
                format!("Historical H {} • base {} • blob {}", seed.head, seed.base, seed.blob.as_deref().unwrap_or("unavailable")),
                "Actual recorded subject FILE; no invented line 1, side, range, thread ID or current path membership.".into(),
            ]);
            observed_login = Some(seed.observed_login.clone());
            if let Some(effect) = &marker.confirmed {
                detail.push(format!(
                    "Recorded observed receipt {:?} #{} • node {} (not proof of causality)",
                    effect.kind,
                    effect.resource_id,
                    effect.node_id.as_deref().unwrap_or("unavailable")
                ));
            }
            let key = seed.draft_key().map_err(source_error)?;
            let encoded = Cache::open(database)
                .map_err(source_error)?
                .bounded_draft(intent.scope(), &key, 128 * 1024)
                .map_err(source_error)?;
            if let Some(encoded) = &encoded {
                let (body, reference) =
                    file_comments_v75::decode_draft(encoded).map_err(source_error)?;
                if reference.as_ref() != Some(&origin.reference)
                    || seed
                        .write(&body)
                        .and_then(|write| write.digest())
                        .map_err(source_error)?
                        != marker.payload_sha256
                {
                    return Err(ThreadBrowseFailureV1::Source);
                }
                detail.push("Exact protected LOCAL requested FILE body, not a current server comment or approval:".into());
                detail.extend(body.split('\n').map(str::to_owned));
            } else {
                detail.push(if marker.state == PrWriteState::Confirmed { "Local requested body unavailable; may have been removed after confirmation. Receipt remains inspectable." } else { "Local requested body unavailable; persistence/ACK is UNKNOWN, not an empty body or resend grant." }.into());
            }
            body_witness = Some((key, encoded));
        } else {
            detail.push("Protected FILE legacy record: exact original metadata unavailable. No body, path, target, line or permission is guessed.".into());
        }
    }
    let labels = snapshot
        .rows
        .iter()
        .map(|row| {
            escape(&format!(
                "#{} {:?} FILE • {}",
                row.marker.attempt_id,
                row.marker.state,
                if row.origin.is_some() {
                    "exact original metadata"
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
    let header = vec![
        escape(&format!("Fresh current {}#{} • repo #{} / PR #{}", parent.repository(), parent.number(), parent.repository_id(), parent.database_id())),
        escape(&format!("Current H {} • base {} (not historical H)", parent.head_sha(), parent.base_tip_sha())),
        format!("Latest {} FILE attempts • {} • current numeric inspector {:?}", snapshot.rows.len(), if snapshot.complete { "complete bounded enumeration" } else { "further rows UNKNOWN" }, identity.account_id),
        "Read-only history. Historical FILE subject has NO line/range and grants no current membership or write authority.".into(),
    ];
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
        PrWriteJournal::open(database)
            .map_err(|_| safe_local())?
            .validate_file_comment_history_v75(
                &target,
                parent.repository_id(),
                parent.repository_node(),
                parent.database_id(),
                &snapshot,
            )
            .map_err(|_| safe_local())?;
        if let Some((key, expected)) = &body_witness {
            if Cache::open(database)
                .map_err(|_| safe_local())?
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
    let actor = ActorIdentity::github(
        &identity.host,
        &crate::providers::gh::Actor {
            login: observed_login.unwrap_or_else(|| {
                if opened.is_none() {
                    identity.login.clone()
                } else {
                    String::new()
                }
            }),
            actor_type: "User".into(),
            ..Default::default()
        },
    );
    Ok(Arc::new(PreparedFileCommentHistoryV75 {
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
