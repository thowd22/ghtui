//! Independent native oracle DATA, not a producer/source/receipt factory.
//! The expected image starts before Confirm. No observed poststate payload,
//! generation, hash, ID or row membership becomes an expected value.
use super::*;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};

const NODE: &str = "GENUINE_CANONICAL_PR_NODE_1007";
const REPO_NODE: &str = "REPO_catalogue_10000";
const UNKNOWN_LABELS: &str =
    "Parent labels unavailable in compatibility PR snapshot; no empty-label state inferred";
const SENTINEL: &str = "state-confirm-oracle-preserved-v86";

/// Fixture DATA only, before the genuine app/Source exists. No queued source,
/// reservation, binding, owner, capability or publication is manufactured.
pub(super) fn seed_preserved_rows(f: &Fixture) -> (u64, u64) {
    let start = now();
    let cache = Cache::open(&database(f)).unwrap();
    for uid in [42, 43] {
        let scope = Scope::github(HOST, Some(uid)).unwrap();
        cache
            .put_draft(
                &scope,
                SENTINEL,
                &format!("Exact unrelated draft {uid} • 界"),
            )
            .unwrap();
        cache
            .save_view(
                &scope,
                SENTINEL,
                &json!({"fixture":"unrelated saved intent","uid":uid,"value":"café 雪"}),
            )
            .unwrap();
        cache
            .put(
                &scope,
                &ghtui::storage::Record {
                    kind: SENTINEL.into(),
                    id: SENTINEL.into(),
                    repository: "unrelated/preserved".into(),
                    payload: json!({"fixture":"unrelated cached data","uid":uid,"value":"café 雪"}),
                    fetched_at: 123,
                    source_updated_at: Some("2026-10-01T00:00:00Z".into()),
                    complete: true,
                },
            )
            .unwrap();
    }
    (start, now())
}
fn assert_preserved_rows(rows: &SqlRows, times: (u64, u64)) {
    for uid in [42, 43] {
        let login = format!("user-id:{uid}");
        let drafts = rows["drafts"]
            .iter()
            .filter(|r| r[0] == t(HOST) && r[1] == t(&login) && r[2] == t(SENTINEL))
            .collect::<Vec<_>>();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].len(), 5);
        assert_eq!(
            drafts[0][..4],
            [
                t(HOST),
                t(&login),
                t(SENTINEL),
                t(&format!("Exact unrelated draft {uid} • 界"))
            ]
        );
        let updated = integer(&drafts[0][4]);
        assert!(updated >= 0 && (times.0..=times.1).contains(&(updated as u64)));
        let expected = [
            (
                "saved_views",
                vec![
                    t(HOST),
                    t(&login),
                    t(SENTINEL),
                    t(
                        &json!({"fixture":"unrelated saved intent","uid":uid,"value":"café 雪"})
                            .to_string(),
                    ),
                ],
            ),
            (
                "records",
                vec![
                    t(HOST),
                    t(&login),
                    t(SENTINEL),
                    t(SENTINEL),
                    t("unrelated/preserved"),
                    t(
                        &json!({"fixture":"unrelated cached data","uid":uid,"value":"café 雪"})
                            .to_string(),
                    ),
                    S::Integer(123),
                    t("2026-10-01T00:00:00Z"),
                    S::Integer(1),
                ],
            ),
        ];
        for (table, expected) in expected {
            assert_eq!(rows[table].iter().filter(|row|*row==&expected).count(),1,
                "Nonvacuous exact {uid} {table} sentinel must exist before Confirm and remain unchanged");
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct FileImage {
    path: Vec<u8>,
    device: u64,
    inode: u64,
    uid: u32,
    mode: u32,
    links: u64,
}
fn file_image(f: &Fixture) -> FileImage {
    let path = database(f);
    let named = fs::symlink_metadata(&path).unwrap();
    let canonical = fs::canonicalize(path).unwrap();
    let actual = fs::symlink_metadata(&canonical).unwrap();
    assert!(named.is_file() && actual.is_file());
    assert_eq!((named.dev(), named.ino()), (actual.dev(), actual.ino()));
    assert_eq!(actual.mode() & 0o777, 0o600);
    assert_eq!(actual.nlink(), 1);
    FileImage {
        path: canonical.as_os_str().as_bytes().to_vec(),
        device: actual.dev(),
        inode: actual.ino(),
        uid: actual.uid(),
        mode: actual.mode(),
        links: actual.nlink(),
    }
}
fn schema(f: &Fixture) -> Vec<(String, String, String, Option<String>)> {
    raw(f)
        .prepare("SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name,tbl_name")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}
fn sequence(f: &Fixture) -> Vec<(String, i64)> {
    raw(f)
        .prepare("SELECT name,seq FROM sqlite_sequence ORDER BY name")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
fn hash(value: &impl Serialize) -> String {
    format!("{:x}", Sha256::digest(serde_json::to_vec(value).unwrap()))
}
fn integer(value: &S) -> i64 {
    match value {
        S::Integer(value) => *value,
        _ => panic!("Expected actual integer SQL value, got {value:?}"),
    }
}
fn text(value: &S) -> &str {
    match value {
        S::Text(value) => value,
        _ => panic!("Expected actual text SQL value, got {value:?}"),
    }
}
fn owned(row: &[S]) -> bool {
    row[0] == t(HOST) && row[1] == t("user-id:42")
}
fn canonical(rows: &mut SqlRows) {
    for rows in rows.values_mut() {
        rows.sort_by_cached_key(|row| format!("{row:?}"));
    }
}

// Exact serialized wire order is intentional: these independent test mirrors
// contain DATA ONLY. Never pass them into a production origin/permit API.
#[derive(Serialize)]
struct DatabaseData<'a> {
    path: &'a [u8],
    device: u64,
    inode: u64,
}
#[derive(Serialize)]
struct SeedData<'a> {
    version: u8,
    database: DatabaseData<'a>,
    scope: &'a Scope,
    repository_id: u64,
    repository_node: &'a str,
    repository: &'a str,
    pr_id: u64,
    pr_node: &'a str,
    number: u64,
    issue_id: u64,
    issue_node: &'a str,
    head: &'a str,
    base: &'a str,
    head_ref: &'a str,
    base_ref: &'a str,
    before: &'a str,
    draft: bool,
    command: ghtui::pr_state_model::PrStateCommand,
    requester: &'a str,
    lineage_v68: Option<()>,
}
#[derive(Serialize)]
struct OriginData<'a> {
    seed: &'a SeedData<'a>,
    reference: &'a ghtui::pr_write_journal::PrPublicationReference,
}
#[derive(Serialize)]
struct DispositionData<'a> {
    version: u8,
    command: ghtui::pr_state_model::PrStateCommand,
    reference: &'a ghtui::pr_write_journal::PrPublicationReference,
    repository_id: u64,
    pr_id: u64,
    issue_id: u64,
    old_state: &'a str,
    new_state: &'a str,
    head: &'a str,
    base: &'a str,
    head_ref: &'a str,
    base_ref: &'a str,
    draft: bool,
    merged: bool,
}
fn framed_hash(row: &[S]) -> String {
    let mut digest = Sha256::new();
    for value in row {
        let len = match value {
            S::Null => 0,
            S::Integer(_) | S::Real(_) => 8,
            S::Text(value) => value.len(),
            S::Blob(value) => value.len(),
        };
        digest.update((len as u64).to_le_bytes());
        match value {
            S::Null => digest.update([0]),
            S::Integer(value) => {
                digest.update([1]);
                digest.update(value.to_le_bytes());
            }
            S::Real(value) => {
                digest.update([2]);
                digest.update(value.to_bits().to_le_bytes());
            }
            S::Text(value) => {
                digest.update([3]);
                digest.update(value.as_bytes());
            }
            S::Blob(value) => {
                digest.update([4]);
                digest.update(value);
            }
        }
    }
    format!("{:x}", digest.finalize())
}

/// Exact expected paid response converted into public DATA models only. The
/// production response parser/projection builder is deliberately not called.
fn expected_details() -> ghtui::providers::gh::PrDetails {
    use ghtui::providers::gh::{Actor, ChangedFile, Coverage, PrDetails, PrSummary, PullRequest};
    PrDetails {
        summary: PrSummary {
            pr: PullRequest {
                host: HOST.into(),
                id: NODE.into(),
                repository: FIRST.into(),
                number: 7,
                title: DETAIL_TITLE.into(),
                body: std::iter::once(DETAIL_BODY.to_owned())
                    .chain((0..32).map(|i| format!("Observed detail line {i:02} • 界🦀")))
                    .collect::<Vec<_>>()
                    .join("\n"),
                url: format!("https://{HOST}/{FIRST}/pull/7"),
                author: Actor {
                    login: "actual-pr-author".into(),
                    display_name: None,
                    avatar_url: None,
                    actor_type: "User".into(),
                },
                state: "closed".into(),
                draft: false,
                created_at: "2026-10-01T00:00:00Z".into(),
                updated_at: "2026-10-14T00:00:00Z".into(),
                queues: Default::default(),
            },
            head_sha: "a".repeat(40),
            base_sha: "b".repeat(40),
            head_ref: "feature/scoped-open".into(),
            base_ref: "main".into(),
            merged: false,
            auto_merge: false,
            merge_commit_sha: None,
            mergeable: None,
            mergeable_state: "unknown".into(),
            additions: 2,
            deletions: 1,
            changed_files: 1,
        },
        comments: vec![],
        commits: vec![],
        files: vec![ChangedFile {
            path: "src/界.rs".into(),
            previous_path: None,
            status: "modified".into(),
            additions: 2,
            deletions: 1,
            patch: Some("@@ -1 +1 @@\n-old\n+new".into()),
        }],
        reviews: vec![],
        timeline: vec![],
        coverage: Coverage {
            loaded: 1,
            total_matches: 1,
            complete: true,
            reasons: vec![],
        },
    }
}

struct ExpectedRecord {
    kind: &'static str,
    id: String,
    payload: Value,
    fetched: u64,
    updated: Option<String>,
    complete: bool,
}
fn expected_discussion(
    scope: &Scope,
    category: ghtui::search::model::DiscussionKind,
    fetched: u64,
) -> (String, Vec<ExpectedRecord>) {
    use ghtui::search::model::{
        Destination, DiscussionContext, Document, DocumentKey, Metadata, ResultKind, SourceKind,
    };
    let body = category == ghtui::search::model::DiscussionKind::PullRequestBody;
    // Public data shapes only, not DiscussionTarget::source/binding/builders.
    let source = format!(
        "collaboration-v1:{}",
        hash(&(
            scope,
            10000u64,
            FIRST.to_ascii_lowercase(),
            "PullRequest",
            NODE,
            category
        ))
    );
    let target = json!({"scope":scope,"repository_id":10000,"repository":FIRST,
        "parent_kind":"PullRequest","parent_node":NODE,"number":7,"category":category});
    let mut records = Vec::new();
    if body {
        let details = expected_details();
        let pr = &details.summary.pr;
        let key = DocumentKey {
            scope: scope.clone(),
            repository: FIRST.into(),
            revision: "discussion".into(),
            kind: ResultKind::PullRequest,
            path: None,
            resource: format!(
                "discussion:{}",
                hash(&(10000u64, "PullRequest", NODE, category, NODE))
            ),
        };
        let document = Document {
            key: key.clone(),
            source: SourceKind::Collaboration,
            title: pr.title.clone(),
            body: pr.body.clone(),
            metadata: Metadata {
                actors: vec![pr.author.clone()],
                state: Some("closed".into()),
                created_at: Some(pr.created_at.clone()),
                updated_at: Some(pr.updated_at.clone()),
                fetched_at: fetched,
                complete: false,
                discussion: Some(DiscussionContext {
                    kind: category,
                    repository_id: 10000,
                    parent_node: NODE.into(),
                    parent_number: 7,
                    resource_node: NODE.into(),
                    resource_numeric_id: None,
                    thread_id: None,
                    path: None,
                    commit_sha: None,
                    side: None,
                    line: None,
                    start_line: None,
                    outdated: false,
                    resolved: None,
                    url: Some(pr.url.clone()),
                }),
                ..Default::default()
            },
            destination: Destination::PullRequest {
                host: HOST.into(),
                repository: FIRST.into(),
                number: 7,
                comment_id: None,
                thread_id: None,
            },
        };
        records.push(ExpectedRecord {
            kind: "collaboration-document-v1",
            id: hash(&key),
            payload: serde_json::to_value(document).unwrap(),
            fetched,
            updated: Some(pr.updated_at.clone()),
            complete: false,
        });
    }
    records.push(ExpectedRecord {
        kind: "collaboration-state-v1",
        id: source.clone(),
        payload: json!({"target":target,"access":"Allowed","enumeration_complete":true,
            "complete":false,"reasons":[UNKNOWN_LABELS],"fetched_at":fetched,"head_sha":"a".repeat(40),
            "suppressed_resources":[],"suppressed_threads":[]}),
        fetched,
        updated: None,
        complete: false,
    });
    (source, records)
}

fn generation(rows: &SqlRows, source: &str) -> i64 {
    let old = rows["sync_reservations"]
        .iter()
        .find(|r| owned(r) && r[2] == t(source))
        .map(|r| integer(&r[3]) + 1)
        .unwrap_or(0);
    let checkpoint = rows["checkpoints"]
        .iter()
        .find(|r| owned(r) && r[2] == t(source))
        .map(|r| integer(&r[3]))
        .unwrap_or(0);
    old.max(checkpoint) + 1
}
fn lookup_events(rows: &mut SqlRows, namespace: &str, repository: &str, events: i64) {
    assert!(events > 0);
    let epochs = rows.get_mut("sync_binding_lookup_epochs").unwrap();
    if let Some(row) = epochs
        .iter_mut()
        .find(|r| owned(r) && r[2] == t(namespace) && r[3] == t(repository))
    {
        row[4] = S::Integer(integer(&row[4]) + events);
    } else {
        epochs.push(vec![
            t(HOST),
            t("user-id:42"),
            t(namespace),
            t(repository),
            S::Integer(events),
        ]);
    }
}
fn assert_first_bound_source(before: &SqlRows, source: &str) {
    for table in ["sync_reservations", "sync_resource_bindings", "checkpoints"] {
        assert!(
            before[table]
                .iter()
                .filter(|r| owned(r))
                .all(|r| r[2] != t(source)),
            "Actual first Confirm PRE image has no old {table} row for {source}"
        );
    }
    assert!(before["checkpoint_records"]
        .iter()
        .filter(|r| owned(r))
        .all(|r| r[2] != t(source)));
    assert!(before["checkpoint_records"]
        .iter()
        .filter(|r| owned(r) && r[3] == t("collaboration-state-v1"))
        .all(|r| r[4] != t(source)));
    assert!(before["records"]
        .iter()
        .filter(|r| owned(r) && r[2] == t("collaboration-state-v1"))
        .all(|r| r[3] != t(source)));
}
fn replace_source(
    rows: &mut SqlRows,
    source: &str,
    generation: i64,
    fetched: u64,
    records: Vec<ExpectedRecord>,
    complete: bool,
) {
    let previous = rows["checkpoint_records"]
        .iter()
        .filter(|r| owned(r) && r[2] == t(source))
        .map(|r| (r[3].clone(), r[4].clone()))
        .collect::<Vec<_>>();
    let memberships = rows.get_mut("checkpoint_records").unwrap();
    memberships.retain(|r| !(owned(r) && r[2] == t(source)));
    for record in &records {
        memberships.push(vec![
            t(HOST),
            t("user-id:42"),
            t(source),
            t(record.kind),
            t(&record.id),
        ]);
    }
    let retained = memberships.clone();
    let stored = rows.get_mut("records").unwrap();
    stored.retain(|r| {
        !owned(r)
            || !previous.contains(&(r[2].clone(), r[3].clone()))
            || retained
                .iter()
                .any(|m| owned(m) && m[3] == r[2] && m[4] == r[3])
    });
    for record in records {
        stored.retain(|r| !(owned(r) && r[2] == t(record.kind) && r[3] == t(&record.id)));
        stored.push(vec![
            t(HOST),
            t("user-id:42"),
            t(record.kind),
            t(&record.id),
            t(FIRST),
            t(&serde_json::to_string(&record.payload).unwrap()),
            S::Integer(record.fetched as i64),
            record.updated.as_deref().map_or(S::Null, t),
            S::Integer(i64::from(record.complete)),
        ]);
    }
    let checkpoints = rows.get_mut("checkpoints").unwrap();
    checkpoints.retain(|r| !(owned(r) && r[2] == t(source)));
    checkpoints.push(vec![
        t(HOST),
        t("user-id:42"),
        t(source),
        S::Integer(generation),
        S::Integer(fetched as i64),
        S::Integer(i64::from(complete)),
    ]);
    let reservations = rows.get_mut("sync_reservations").unwrap();
    reservations.retain(|r| !(owned(r) && r[2] == t(source)));
    reservations.push(vec![
        t(HOST),
        t("user-id:42"),
        t(source),
        S::Integer(generation),
        S::Integer(1),
    ]);
}

pub(super) struct ConfirmSqlV86 {
    before: SqlRows,
    schema: Vec<(String, String, String, Option<String>)>,
    sequence: Vec<(String, i64)>,
    file: FileImage,
    start: u64,
    sentinel_times: (u64, u64),
    key: String,
    catalogue: Option<String>,
}
impl ConfirmSqlV86 {
    pub(super) fn capture(f: &Fixture, personal: bool, sentinel_times: (u64, u64)) -> Self {
        let before = all_rows(f);
        assert_preserved_rows(&before, sentinel_times);
        for name in [
            "pr_write_attempts",
            "pr_write_state_origins_v68",
            "pr_write_state_dispositions_v68",
            "pr_write_state_filter_fences_v68",
            "pr_write_state_source_watermarks_v73",
        ] {
            assert!(
                before[name].is_empty(),
                "Genuine first State fixture has no old {name}"
            );
        }
        let scope = Scope::github(HOST, Some(42)).unwrap();
        let command = ghtui::pr_state_model::PrStateCommand::Close;
        let stable = (&scope, 10000u64, REPO_NODE, 1007u64, NODE, 7u64, command);
        let key = format!(
            "state-v68/{}",
            hash(&(stable, "a".repeat(40), "b".repeat(40)))
        );
        let drafts = before["drafts"]
            .iter()
            .filter(|r| owned(r) && text(&r[2]).starts_with("state-v68/"))
            .collect::<Vec<_>>();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0][2], t(&key));
        assert_eq!(
            drafts[0][3],
            t("{\"version\":17,\"values\":{},\"publication\":null}")
        );
        let catalogue = before["sync_reservations"]
            .iter()
            .filter(|r| owned(r) && text(&r[2]).starts_with("repository-catalogue-source-v1:"))
            .map(|r| text(&r[2]).to_owned())
            .collect::<Vec<_>>();
        assert_eq!(catalogue.len(), usize::from(!personal));
        Self {
            before,
            schema: schema(f),
            sequence: sequence(f),
            file: file_image(f),
            start: now(),
            sentinel_times,
            key,
            catalogue: catalogue.into_iter().next(),
        }
    }
    /// No remote reads or SQL writes. Only named clock samples are observations;
    /// every other expected byte is fixed fixture data plus the PRE image.
    pub(super) fn assert_confirmed(&self, f: &Fixture) -> SqlRows {
        let scope = Scope::github(HOST, Some(42)).unwrap();
        let command = ghtui::pr_state_model::PrStateCommand::Close;
        let head = "a".repeat(40);
        let base = "b".repeat(40);
        let id = self
            .sequence
            .iter()
            .find(|(name, _)| name == "pr_write_attempts")
            .map_or(1, |(_, n)| n + 1);
        let projection = format!("pr-write-projection:{NODE}");
        let db = raw(f);
        let created: i64 = db
            .query_row(
                "SELECT created_at FROM pr_write_attempts WHERE attempt_id=?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        let fetched:i64 = db.query_row("SELECT fetched_at FROM checkpoints WHERE host=?1 AND login='user-id:42' AND source=?2",[HOST,projection.as_str()],|r|r.get(0)).unwrap();
        let record_time:i64 = db.query_row("SELECT fetched_at FROM records WHERE host=?1 AND login='user-id:42' AND kind='pr-details' AND id=?2",[HOST,NODE],|r|r.get(0)).unwrap();
        let end = now();
        for (label, value) in [
            ("attempt creation", created),
            ("derived batch", fetched),
            ("projection record", record_time),
        ] {
            assert!(
                value >= 0 && (self.start..=end).contains(&(value as u64)),
                "Only bounded integer {label} clock may be observed"
            );
        }
        assert!(created <= fetched && fetched <= record_time);
        let fetched = fetched as u64;
        let seed = SeedData {
            version: 68,
            database: DatabaseData {
                path: &self.file.path,
                device: self.file.device,
                inode: self.file.inode,
            },
            scope: &scope,
            repository_id: 10000,
            repository_node: REPO_NODE,
            repository: FIRST,
            pr_id: 1007,
            pr_node: NODE,
            number: 7,
            issue_id: 2007,
            issue_node: "INDEPENDENT_ISSUE_2007",
            head: &head,
            base: &base,
            head_ref: "feature/scoped-open",
            base_ref: "main",
            before: "open",
            draft: false,
            command,
            requester: "octocat",
            lineage_v68: None,
        };
        let digest = hash(&(
            "state-close",
            &self.key,
            &head,
            BTreeMap::from([("origin-v68", hash(&seed))]),
        ));
        let reference = ghtui::pr_write_journal::PrPublicationReference {
            attempt_id: id,
            pr_node_id: NODE.into(),
            observed_head: head.clone(),
            payload_sha256: digest.clone(),
        };
        let origin = serde_json::to_string(&OriginData {
            seed: &seed,
            reference: &reference,
        })
        .unwrap();
        let receipt = serde_json::to_string(&DispositionData {
            version: 68,
            command,
            reference: &reference,
            repository_id: 10000,
            pr_id: 1007,
            issue_id: 2007,
            old_state: "open",
            new_state: "closed",
            head: &head,
            base: &base,
            head_ref: "feature/scoped-open",
            base_ref: "main",
            draft: false,
            merged: false,
        })
        .unwrap();
        let base_key = format!(
            "state-base-v68/{}",
            hash(&(&scope, 10000u64, REPO_NODE, 1007u64, NODE, 7u64, command))
        );
        let origin_row = vec![
            S::Integer(id),
            S::Integer(10000),
            S::Integer(1007),
            S::Integer(2007),
            t("state-close"),
            t(&base_key),
            t("state-close"),
            S::Null,
            t(&origin),
        ];
        let disposition_row = vec![S::Integer(id), t("confirmed_effect"), t(&receipt)];
        let origin_hash = framed_hash(&origin_row);
        let disposition_hash = framed_hash(&disposition_row);
        let effect = ghtui::pr_write_journal::PrConfirmedEffect {
            kind: ghtui::pr_write_journal::PrEffectKind::Lifecycle,
            resource_id: "state-v68/close/closed".into(),
            node_id: Some(NODE.into()),
        };
        let mut expected = self.before.clone();
        // The State scope fence updates each actual PRE reservation OLD+NEW
        // and each PRE binding's active/visible fields once. Derive grouped
        // lookup keys from the original typed rows, not an observed epoch.
        let mut scope_lookup = BTreeMap::<(String, String), i64>::new();
        for binding in self.before["sync_resource_bindings"]
            .iter()
            .filter(|r| owned(r))
        {
            let reserved = self.before["sync_reservations"]
                .iter()
                .any(|r| owned(r) && r[2] == binding[2]);
            *scope_lookup
                .entry((text(&binding[3]).into(), text(&binding[5]).into()))
                .or_default() += 2 + 2 * i64::from(reserved);
        }
        for ((namespace, repository), events) in scope_lookup {
            lookup_events(&mut expected, &namespace, &repository, events);
        }
        expected.get_mut("pr_write_attempts").unwrap().push(vec![
            S::Integer(id),
            t(HOST),
            t("user-id:42"),
            t(NODE),
            t(FIRST),
            S::Integer(7),
            t("state-close"),
            t(&self.key),
            t(&head),
            t(&digest),
            t("confirmed"),
            t(&serde_json::to_string(&effect).unwrap()),
            S::Integer(created),
        ]);
        expected
            .get_mut("pr_write_state_origins_v68")
            .unwrap()
            .push(origin_row);
        expected
            .get_mut("pr_write_state_dispositions_v68")
            .unwrap()
            .push(disposition_row);
        let scopes = expected.get_mut("pr_write_scopes").unwrap();
        if let Some(row) = scopes.iter_mut().find(|r| owned(r) && r[2] == t(NODE)) {
            row[3] = S::Integer(integer(&row[3]) + 3);
        } else {
            scopes.push(vec![t(HOST), t("user-id:42"), t(NODE), S::Integer(3)]);
        }
        for row in expected
            .get_mut("sync_reservations")
            .unwrap()
            .iter_mut()
            .filter(|r| owned(r))
        {
            row[3] = S::Integer(integer(&row[3]) + 1);
            row[4] = S::Integer(0);
        }
        for row in expected
            .get_mut("sync_resource_bindings")
            .unwrap()
            .iter_mut()
            .filter(|r| owned(r))
        {
            row[9] = S::Null;
            row[10] = S::Integer(0);
        }
        if let Some(source) = &self.catalogue {
            let row = expected
                .get_mut("sync_reservations")
                .unwrap()
                .iter_mut()
                .find(|r| owned(r) && r[2] == t(source))
                .unwrap();
            row[3] = S::Integer(integer(&row[3]) + 1);
            row[4] = S::Integer(1);
        }
        expected
            .get_mut("pr_write_state_filter_fences_v68")
            .unwrap()
            .push(vec![
                t(HOST),
                t("user-id:42"),
                t(NODE),
                S::Integer(10000),
                S::Integer(1007),
                t(FIRST),
                S::Integer(id),
                t(&origin_hash),
                t(&disposition_hash),
                t("unavailable"),
            ]);
        let mut sources = Vec::new();
        for category in [
            ghtui::search::model::DiscussionKind::PullRequestBody,
            ghtui::search::model::DiscussionKind::ConversationComment,
        ] {
            let (source, records) = expected_discussion(&scope, category, fetched);
            // Positive first-publication facts, not a zero-epoch default:
            // neither this binding/source nor its state row/membership existed
            // in the captured PRE image. Unexpected fixture state must fail.
            assert_first_bound_source(&self.before, &source);
            assert_eq!(
                records
                    .iter()
                    .filter(|r| r.kind == "collaboration-state-v1" && r.id == source)
                    .count(),
                1,
                "Exactly one independently derived state record per actual category"
            );
            let next = generation(&self.before, &source);
            replace_source(&mut expected, &source, next, fetched, records, false);
            let bindings = expected.get_mut("sync_resource_bindings").unwrap();
            assert!(bindings.iter().all(|r| !owned(r) || r[2] != t(&source)));
            bindings.push(vec![
                t(HOST),
                t("user-id:42"),
                t(&source),
                t("collaboration"),
                S::Integer(10000),
                t(FIRST),
                t(NODE),
                t(
                    if category == ghtui::search::model::DiscussionKind::PullRequestBody {
                        "pull_request_body"
                    } else {
                        "conversation_comment"
                    },
                ),
                S::Integer(next),
                S::Integer(next),
                S::Integer(1),
            ]);
            // Actual producer order: reservation INSERT precedes the new
            // binding and therefore has no lookup match. Thereafter these
            // exact SQL events each fire once for this actual bound source.
            // Other non-state records do not satisfy the trigger's WHEN.
            for _event in [
                "INSERT binding",
                "INSERT collaboration-state record",
                "INSERT collaboration-state membership",
                "INSERT checkpoint",
                "UPDATE binding visible generation",
            ] {
                lookup_events(&mut expected, "collaboration", FIRST, 1);
            }
            sources.push((source, next));
        }
        let next = generation(&self.before, &projection);
        let details = expected_details();
        replace_source(
            &mut expected,
            &projection,
            next,
            fetched,
            vec![ExpectedRecord {
                kind: "pr-details",
                id: NODE.into(),
                payload: serde_json::to_value(&details).unwrap(),
                fetched: record_time as u64,
                updated: Some(details.summary.pr.updated_at),
                complete: true,
            }],
            true,
        );
        sources.push((projection, next));
        let watermarks = expected
            .get_mut("pr_write_state_source_watermarks_v73")
            .unwrap();
        for (source, next) in &sources {
            watermarks.push(vec![
                t(HOST),
                t("user-id:42"),
                t(NODE),
                t(source),
                S::Integer(*next),
                S::Integer(id),
                S::Integer(10000),
                S::Integer(1007),
            ]);
        }
        let drafts = expected.get_mut("drafts").unwrap();
        let count = drafts.len();
        drafts.retain(|r| !(owned(r) && r[2] == t(&self.key)));
        assert_eq!(
            drafts.len() + 1,
            count,
            "Only own exact bodyless draft is deleted"
        );
        canonical(&mut expected);
        assert_preserved_rows(&expected, self.sentinel_times);
        assert_eq!(all_rows(f),expected,"ENTIRE independent pre-Confirm -> exact typed Confirm SQL image, no poststate baseline/exclusions");
        self.assert_auxiliary_unchanged(f);
        let cache = Cache::open(&database(f)).unwrap();
        let old_queue = source_name(f);
        assert!(
            cache.sync_frame(&scope, &old_queue).is_err(),
            "Old queue/count alias is unavailable until its own new genuine complete source"
        );
        assert_eq!(
            all_rows(f),
            expected,
            "Read-only alias refusal changes no SQL/Clear witness"
        );
        self.assert_auxiliary_unchanged(f);
        expected
    }
    /// Source Refresh and Back may change only the independently derived row
    /// image. The original schema/private file and one-attempt sequence remain
    /// exact through both real refreshes and the final consuming Quit.
    pub(super) fn assert_auxiliary_unchanged(&self, f: &Fixture) {
        let id = self
            .sequence
            .iter()
            .find(|(name, _)| name == "pr_write_attempts")
            .map_or(1, |(_, n)| n + 1);
        let mut expected_sequence = self.sequence.clone();
        if let Some((_, value)) = expected_sequence
            .iter_mut()
            .find(|(name, _)| name == "pr_write_attempts")
        {
            *value = id;
        } else {
            expected_sequence.push(("pr_write_attempts".into(), id));
        }
        expected_sequence.sort();
        assert_eq!(sequence(f), expected_sequence);
        assert_eq!(schema(f), self.schema);
        assert_eq!(file_image(f), self.file);
    }
}
