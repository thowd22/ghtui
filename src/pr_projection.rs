//! Derived discussion records published only after a confirmed PR write and a
//! fresh, account-verified server snapshot. The durable receipt remains separate.
use crate::{
    github_workspace::GithubWorkspace,
    pr_write_journal::{PrPublicationReference, PrWriteJournal, PrWriteTarget},
    storage::{Cache, Record},
};
use serde_json::json;
use std::{collections::BTreeSet, path::Path};
mod discussion_invalidation;
mod review_impact_v2;
pub(crate) mod review_operation_impact_v3;
pub(crate) mod review_recovery_v11;
pub mod review_rerequest_impact_v3;
pub(crate) mod thread_comment_impact_v2;
pub(crate) mod thread_impact_v1;
pub(crate) mod thread_reaction_impact_v3;
pub(crate) use discussion_invalidation::unresolved_visibility_diagnostic;
pub use discussion_invalidation::{
    confirmed_discussion_impact, guard_discussion_reads, invalidate_confirmed_discussions,
    invalidate_unresolved_discussions, validate_discussion_read_guard, DiscussionReadGuard,
    PrDiscussionImpact,
};

/// Atomically advance this PR's write-derived source. Ordinary reads never use
/// this namespace, and a late completion cannot overwrite a newer publication.
/// Failure affects only derived data: callers must still report the remote
/// effect as confirmed and must never offer a resend.
pub fn persist_confirmed_snapshot(
    database: &Path,
    workspace: &GithubWorkspace,
    publication: &PrPublicationReference,
) -> Result<(), String> {
    let details = &workspace
        .pr
        .as_ref()
        .ok_or("A confirmed projection needs the active exact PR snapshot")?
        .details;
    let target = PrWriteTarget::new(&workspace.identity, &details.summary)
        .map_err(|error| error.to_string())?;
    let journal = PrWriteJournal::open(database).map_err(|error| error.to_string())?;
    journal
        .confirmed_publication(&target, publication)
        .map_err(|error| error.to_string())?
        .ok_or("No confirmed receipt matches this account, canonical PR and original attempt")?;
    if !matches!(details.summary.head_sha.len(), 40 | 64)
        || !details
            .summary
            .head_sha
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("Fresh PR snapshot has no immutable head identity".into());
    }
    let complete = details.coverage.complete;
    let mut record = Record::new(
        "pr-details",
        target.node_id(),
        target.repository(),
        serde_json::to_value(details).map_err(|error| error.to_string())?,
    );
    record.source_updated_at = Some(details.summary.pr.updated_at.clone());
    record.complete = complete;
    let mut records = vec![record];
    let mut comments = BTreeSet::new();
    for comment in &details.comments {
        if comment.id == 0 || comment.node_id.is_empty() || !comments.insert(&comment.node_id) {
            return Err(
                "Projection requires distinct canonical conversation comment identities".into(),
            );
        }
        let mut record = Record::new(
            "pr-conversation-comment",
            &comment.node_id,
            target.repository(),
            json!({
                "version":1,
                "parent_pr_node":target.node_id(),
                "repository":target.repository(),
                "number":target.number(),
                "head_sha":details.summary.head_sha,
                "comment_id":comment.id,
                "comment_node":comment.node_id,
                "body":comment.body,
                "actor":comment.author,
                "created_at":comment.created_at,
                "updated_at":comment.updated_at,
                "url":comment.url,
            }),
        );
        record.source_updated_at = Some(comment.updated_at.clone());
        record.complete = complete;
        records.push(record);
    }
    let mut cache = Cache::open(database).map_err(|error| error.to_string())?;
    let published = cache
        .commit_sync(
            target.scope(),
            &format!("pr-write-projection:{}", target.node_id()),
            publication.attempt_id as u64,
            &records,
            complete,
        )
        .map_err(|error| error.to_string())?;
    if !published {
        return Err("A newer or equal confirmed PR projection already exists; stale completion was not applied".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        github_workspace::{PrPage, PrTab},
        pr_write_journal::{PrConfirmedEffect, PrEffectKind, PrWriteIntent},
        providers::gh::{
            Actor, ConversationComment, Coverage, HostIdentity, PrDetails, PrSummary, PullRequest,
        },
        storage::Scope,
    };
    use rusqlite::Connection;
    use std::collections::{BTreeMap, BTreeSet};
    use std::os::unix::fs::PermissionsExt;

    fn private_directory() -> tempfile::TempDir {
        tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap()
    }

    fn workspace() -> GithubWorkspace {
        let mut workspace = GithubWorkspace::empty(
            HostIdentity {
                host: "github.invalid".into(),
                login: "octocat".into(),
                account_id: Some(42),
                gh_version: "2.101.0".into(),
            },
            vec!["example/repo".into()],
            BTreeSet::new(),
        );
        workspace.pr = Some(PrPage {
            details: PrDetails {
                summary: PrSummary {
                    pr: PullRequest {
                        host: "github.invalid".into(),
                        id: "PR_projection_7".into(),
                        repository: "example/repo".into(),
                        number: 7,
                        title: "Projection fixture".into(),
                        body: "Description".into(),
                        url: "https://github.invalid/example/repo/pull/7".into(),
                        author: Actor {
                            login: "octocat".into(),
                            actor_type: "User".into(),
                            ..Default::default()
                        },
                        state: "open".into(),
                        draft: false,
                        created_at: "t0".into(),
                        updated_at: "t0".into(),
                        queues: BTreeSet::new(),
                    },
                    head_sha: "a".repeat(40),
                    base_sha: "b".repeat(40),
                    head_ref: "feature".into(),
                    base_ref: "main".into(),
                    merged: false,
                    auto_merge: false,
                    merge_commit_sha: None,
                    mergeable: Some(true),
                    mergeable_state: "clean".into(),
                    additions: 0,
                    deletions: 0,
                    changed_files: 0,
                },
                comments: vec![comment(10, "Original café discussion 雪")],
                commits: vec![],
                files: vec![],
                reviews: vec![],
                timeline: vec![],
                coverage: Coverage {
                    complete: true,
                    ..Default::default()
                },
            },
            tab: PrTab::Conversation,
            diff: None,
            checks: None,
            rows: vec![],
            context: None,
            review_progress: None,
        });
        workspace
    }
    fn comment(id: u64, body: &str) -> ConversationComment {
        ConversationComment {
            id,
            node_id: format!("IC_{id}"),
            body: body.into(),
            author: Actor {
                login: "octocat".into(),
                actor_type: "User".into(),
                ..Default::default()
            },
            created_at: "t0".into(),
            updated_at: "t0".into(),
            url: format!("https://github.invalid/example/repo/pull/7#issuecomment-{id}"),
        }
    }
    fn confirmed(
        database: &Path,
        workspace: &GithubWorkspace,
        command: &str,
        kind: PrEffectKind,
    ) -> PrPublicationReference {
        let target = PrWriteTarget::new(
            &workspace.identity,
            &workspace.pr.as_ref().unwrap().details.summary,
        )
        .unwrap();
        let intent = PrWriteIntent {
            command: command.into(),
            selected_resource: "pr".into(),
            observed_head: workspace
                .pr
                .as_ref()
                .unwrap()
                .details
                .summary
                .head_sha
                .clone(),
            values: BTreeMap::from([("body".into(), format!("{command} fixture payload"))]),
        };
        let mut journal = PrWriteJournal::open(database).unwrap();
        let permit = journal.begin(&target, &intent).unwrap();
        journal.claim(&permit, &target, &intent).unwrap();
        journal
            .mark_confirmed(
                &permit,
                &PrConfirmedEffect {
                    kind,
                    resource_id: if kind == PrEffectKind::Subscription {
                        "subscribed"
                    } else {
                        "10"
                    }
                    .into(),
                    node_id: (kind != PrEffectKind::Subscription).then(|| "IC_10".into()),
                },
            )
            .unwrap();
        permit.marker().publication_reference()
    }
    fn scope() -> Scope {
        Scope::github("github.invalid", Some(42)).unwrap()
    }
    fn source() -> &'static str {
        "pr-write-projection:PR_projection_7"
    }

    #[test]
    fn conversation_comments_projection_edits_and_deletes_refresh_exact_scoped_records() {
        let directory = private_directory();
        let database = directory.path().join("cache.sqlite3");
        let mut workspace = workspace();
        let initial = confirmed(&database, &workspace, "new-comment", PrEffectKind::Comment);
        persist_confirmed_snapshot(&database, &workspace, &initial).unwrap();
        let cache = Cache::open(&database).unwrap();
        let record = cache
            .get(&scope(), "pr-conversation-comment", "IC_10")
            .unwrap()
            .unwrap();
        assert_eq!(record.payload["version"], 1);
        assert_eq!(record.payload["body"], "Original café discussion 雪");
        assert_eq!(record.payload["parent_pr_node"], "PR_projection_7");
        assert_eq!(record.payload["actor"]["login"], "octocat");
        assert_eq!(record.payload["comment_id"], 10);
        assert_eq!(record.source_updated_at.as_deref(), Some("t0"));
        let page = workspace.pr.as_mut().unwrap();
        page.details.comments[0].body = "Edited exact Unicode bytes 👩‍💻".into();
        page.details.comments[0].updated_at = "t1".into();
        page.details.summary.pr.updated_at = "t1".into();
        let edited = confirmed(&database, &workspace, "edit-comment", PrEffectKind::Comment);
        persist_confirmed_snapshot(&database, &workspace, &edited).unwrap();
        let record = cache
            .get(&scope(), "pr-conversation-comment", "IC_10")
            .unwrap()
            .unwrap();
        assert_eq!(record.payload["body"], "Edited exact Unicode bytes 👩‍💻");
        assert_eq!(record.source_updated_at.as_deref(), Some("t1"));
        let saved: PrDetails = serde_json::from_value(
            cache
                .get(&scope(), "pr-details", "PR_projection_7")
                .unwrap()
                .unwrap()
                .payload,
        )
        .unwrap();
        assert_eq!(saved.comments[0].body, record.payload["body"]);
        workspace.pr.as_mut().unwrap().details.comments.clear();
        let deleted = confirmed(
            &database,
            &workspace,
            "delete-comment",
            PrEffectKind::CommentDeleted,
        );
        persist_confirmed_snapshot(&database, &workspace, &deleted).unwrap();
        assert!(cache
            .get(&scope(), "pr-conversation-comment", "IC_10")
            .unwrap()
            .is_none());
        let frame = cache.sync_frame(&scope(), source()).unwrap().unwrap();
        assert_eq!(frame.generation, deleted.attempt_id as u64);
        assert!(frame.complete);
        assert_eq!(frame.records.len(), 1);
        assert_eq!(frame.records[0].payload["comments"], json!([]));
        assert_eq!(
            Connection::open(&database)
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM pr_write_attempts WHERE state='confirmed'",
                    [],
                    |row| row.get::<_, u64>(0)
                )
                .unwrap(),
            3
        );
    }

    #[test]
    fn conversation_comments_projection_partial_coverage_retains_missing_comments() {
        let directory = private_directory();
        let database = directory.path().join("cache.sqlite3");
        let mut workspace = workspace();
        workspace
            .pr
            .as_mut()
            .unwrap()
            .details
            .comments
            .push(comment(11, "Second comment retained"));
        let first = confirmed(&database, &workspace, "new-comment", PrEffectKind::Comment);
        persist_confirmed_snapshot(&database, &workspace, &first).unwrap();
        workspace.pr.as_mut().unwrap().details.comments.clear();
        workspace.pr.as_mut().unwrap().details.coverage.complete = false;
        let second = confirmed(
            &database,
            &workspace,
            "subscription",
            PrEffectKind::Subscription,
        );
        persist_confirmed_snapshot(&database, &workspace, &second).unwrap();
        let cache = Cache::open(&database).unwrap();
        let frame = cache.sync_frame(&scope(), source()).unwrap().unwrap();
        assert!(!frame.complete);
        assert_eq!(frame.generation, second.attempt_id as u64);
        assert_eq!(frame.records.len(), 3);
        assert_eq!(
            cache
                .get(&scope(), "pr-conversation-comment", "IC_11")
                .unwrap()
                .unwrap()
                .payload["body"],
            "Second comment retained"
        );
        workspace.pr.as_mut().unwrap().details.coverage.complete = true;
        let third = confirmed(
            &database,
            &workspace,
            "delete-comment",
            PrEffectKind::CommentDeleted,
        );
        persist_confirmed_snapshot(&database, &workspace, &third).unwrap();
        assert_eq!(
            cache
                .sync_frame(&scope(), source())
                .unwrap()
                .unwrap()
                .records
                .len(),
            1
        );
        assert!(cache
            .get(&scope(), "pr-conversation-comment", "IC_11")
            .unwrap()
            .is_none());
    }

    #[test]
    fn conversation_comments_projection_rejects_wrong_account_node_and_original_receipt() {
        let directory = private_directory();
        let database = directory.path().join("cache.sqlite3");
        let workspace = workspace();
        let reference = confirmed(&database, &workspace, "new-comment", PrEffectKind::Comment);
        let mut other = workspace.clone();
        other.identity.account_id = Some(43);
        assert!(persist_confirmed_snapshot(&database, &other, &reference).is_err());
        other.identity.account_id = None;
        assert!(persist_confirmed_snapshot(&database, &other, &reference).is_err());
        other = workspace.clone();
        other.pr.as_mut().unwrap().details.summary.pr.id = "PR_other_7".into();
        assert!(persist_confirmed_snapshot(&database, &other, &reference).is_err());
        other = workspace.clone();
        other.identity.host = "enterprise.invalid".into();
        other.pr.as_mut().unwrap().details.summary.pr.host = "enterprise.invalid".into();
        assert!(persist_confirmed_snapshot(&database, &other, &reference).is_err());
        for changed in [
            PrPublicationReference {
                attempt_id: reference.attempt_id + 100,
                ..reference.clone()
            },
            PrPublicationReference {
                observed_head: "c".repeat(40),
                ..reference.clone()
            },
            PrPublicationReference {
                payload_sha256: "f".repeat(64),
                ..reference.clone()
            },
        ] {
            assert!(persist_confirmed_snapshot(&database, &workspace, &changed).is_err());
        }
        let cache = Cache::open(&database).unwrap();
        assert!(cache.list(&scope(), None).unwrap().is_empty());
        assert!(cache
            .list(&Scope::github("github.invalid", Some(43)).unwrap(), None)
            .unwrap()
            .is_empty());
        persist_confirmed_snapshot(&database, &workspace, &reference).unwrap();
        assert_eq!(cache.list(&scope(), None).unwrap().len(), 2);
    }

    #[test]
    fn conversation_comments_projection_pending_attempt_never_publishes_derived_data() {
        let directory = private_directory();
        let database = directory.path().join("cache.sqlite3");
        let workspace = workspace();
        let target = PrWriteTarget::new(
            &workspace.identity,
            &workspace.pr.as_ref().unwrap().details.summary,
        )
        .unwrap();
        let intent = PrWriteIntent {
            command: "new-comment".into(),
            selected_resource: "pr".into(),
            observed_head: "a".repeat(40),
            values: BTreeMap::from([("body".into(), "Pending text".into())]),
        };
        let mut journal = PrWriteJournal::open(&database).unwrap();
        let permit = journal.begin(&target, &intent).unwrap();
        assert!(persist_confirmed_snapshot(
            &database,
            &workspace,
            &permit.marker().publication_reference()
        )
        .is_err());
        assert!(Cache::open(&database)
            .unwrap()
            .list(&scope(), None)
            .unwrap()
            .is_empty());
        assert_eq!(
            journal.blocking(&target).unwrap().unwrap().attempt_id,
            permit.marker().attempt_id
        );
    }

    #[test]
    fn conversation_comments_projection_late_or_equal_generation_cannot_restore_old_body() {
        let directory = private_directory();
        let database = directory.path().join("cache.sqlite3");
        let workspace = workspace();
        let first = confirmed(&database, &workspace, "new-comment", PrEffectKind::Comment);
        let mut updated = workspace.clone();
        updated.pr.as_mut().unwrap().details.comments[0].body = "Newer exact bytes".into();
        let second = confirmed(&database, &updated, "edit-comment", PrEffectKind::Comment);
        persist_confirmed_snapshot(&database, &updated, &second).unwrap();
        let cache = Cache::open(&database).unwrap();
        let expected = cache.sync_frame(&scope(), source()).unwrap();
        assert!(persist_confirmed_snapshot(&database, &workspace, &first).is_err());
        assert!(persist_confirmed_snapshot(&database, &workspace, &second).is_err());
        assert_eq!(cache.sync_frame(&scope(), source()).unwrap(), expected);
        assert_eq!(
            cache
                .get(&scope(), "pr-conversation-comment", "IC_10")
                .unwrap()
                .unwrap()
                .payload["body"],
            "Newer exact bytes"
        );
    }

    #[test]
    fn conversation_comments_projection_storage_failure_is_atomic_and_preserves_confirmed_receipt()
    {
        let directory = private_directory();
        let database = directory.path().join("cache.sqlite3");
        let mut workspace = workspace();
        let first = confirmed(&database, &workspace, "new-comment", PrEffectKind::Comment);
        persist_confirmed_snapshot(&database, &workspace, &first).unwrap();
        let cache = Cache::open(&database).unwrap();
        let expected = cache.sync_frame(&scope(), source()).unwrap();
        workspace.pr.as_mut().unwrap().details.comments[0].body =
            "Confirmed remote effect with failed cache".into();
        let second = confirmed(&database, &workspace, "edit-comment", PrEffectKind::Comment);
        let connection = Connection::open(&database).unwrap();
        connection.execute_batch("CREATE TRIGGER reject_projection BEFORE INSERT ON records WHEN NEW.kind='pr-conversation-comment' BEGIN SELECT RAISE(ABORT,'fixture projection failure'); END;").unwrap();
        assert!(persist_confirmed_snapshot(&database, &workspace, &second)
            .unwrap_err()
            .contains("fixture projection failure"));
        assert_eq!(cache.sync_frame(&scope(), source()).unwrap(), expected);
        assert_eq!(
            connection
                .query_row(
                    "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
                    [second.attempt_id],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "confirmed"
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM pr_write_attempts", [], |row| row
                    .get::<_, u64>(0))
                .unwrap(),
            2
        );
        connection
            .execute_batch("DROP TRIGGER reject_projection")
            .unwrap();
        persist_confirmed_snapshot(&database, &workspace, &second).unwrap();
        assert_eq!(
            cache
                .get(&scope(), "pr-conversation-comment", "IC_10")
                .unwrap()
                .unwrap()
                .payload["body"],
            "Confirmed remote effect with failed cache"
        );
    }
}
