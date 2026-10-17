//! Real same-Core prewrite/claim/RPC/readback. This is receipt-stage evidence,
//! not a substitute for native fresh-page/state-filter/ACK journeys.
use super::*;
use crate::{
    drafts::DraftWriter,
    pr_actions::PrPlan,
    pr_write_journal::state_origin_v68::StateOriginSeedV68,
    pr_write_journal::{PrWriteJournal, PrWriteState},
    scoped_pr_collaboration_source_v17::state_phase_v68::{
        StatePhaseRequestV68, StatePublicationAckV68,
    },
};
pub(super) fn receipt_steps() -> Vec<GhStep> {
    let mut node = capability(json!(true))["data"]["node"].clone();
    node["state"] = json!("CLOSED");
    let mut mutation = get(
        "graphql",
        json!({"data":{"closePullRequest":{"pullRequest":node}}}),
    );
    *mutation
        .args
        .iter_mut()
        .find(|a| a.as_str() == "GET")
        .unwrap() = "POST".into();
    mutation.args.extend(["--input".into(), "-".into()]);
    mutation.stdin=serde_json::to_vec(&json!({"query":"mutation($id:ID!){closePullRequest(input:{pullRequestId:$id}){pullRequest{id fullDatabaseId number headRefOid baseRefOid headRefName baseRefName state isDraft merged repository{id nameWithOwner}}}}","variables":{"id":"PERSONAL_PR_1007"}})).unwrap();
    let mut closed = state_parent();
    closed["state"] = json!("closed");
    vec![mutation, get(&format!("repos/{REPO}/pulls/7"), closed)]
}
pub(super) fn saved_bodyless(writer: &mut DraftWriter, s: &Setup, key: &str) {
    let encoded =
        crate::scoped_pr_collaboration_draft_v17::encode_draft_v17(&BTreeMap::new(), None).unwrap();
    writer
        .save(
            Scope::github("github.invalid", Some(42)).unwrap(),
            key.into(),
            encoded.clone(),
        )
        .unwrap();
    let client = writer.client();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(8);
    let cutoff = client
        .capture_conversation_open_cutoff_v20(&database(&s.fixture), &cancel, deadline)
        .unwrap();
    loop {
        let acks = writer.poll();
        if let Some(ack) = acks.into_iter().next() {
            assert_eq!(ack.resource, key);
            ack.result.unwrap();
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Actual State initial Saved ACK required"
        );
        std::thread::yield_now();
    }
    client
        .wait_conversation_open_cutoff_v20(&cutoff, &cancel)
        .unwrap();
    cutoff.check(&client, &cancel).unwrap();
    assert_eq!(
        Cache::open(&database(&s.fixture))
            .unwrap()
            .draft(&Scope::github("github.invalid", Some(42)).unwrap(), key)
            .unwrap()
            .as_deref(),
        Some(encoded.as_str())
    );
}
#[test]
fn pr_lifecycle_v68_two_actual_claimed_receipts_cannot_graft_or_half_confirm_and_preserve_protected_predecessor(
) {
    two_actual_attempts(0);
}
#[test]
fn pr_lifecycle_v68_same_ordinal_unresolved_fence_advances_only_with_actual_claimed_receipt_without_refencing(
) {
    two_actual_attempts(1);
}
#[test]
fn pr_lifecycle_v68_actual_local_confirmation_failure_retains_uncertainty_and_revokes_only_original_sources(
) {
    two_actual_attempts(2);
}
fn two_actual_attempts(repair_mode: u8) {
    let mut steps = phase(capability(json!(true)));
    for _ in 0..2 {
        steps.extend(phase(capability(json!(true))));
        steps.extend(receipt_steps());
    }
    // The second actual opening observes OPEN again (an independent external
    // reopen). No fixture/UI state is promoted to access or causal attribution.
    let s = setup(steps);
    let original = PrCollaborationOpenIntentV17::capture_state_v68(
        &s.app,
        &database(&s.fixture),
        PrStateCommand::Close,
    )
    .unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    let (source, _, base) = source::state_v68::open_state_v68(
        source::state_v68::StateOpenRequestV68 {
            opening: Arc::clone(&original),
            command: PrStateCommand::Close,
        },
        &provider,
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
        |_, _, base| Ok(base.unwrap().clone()),
        |_| Ok(()),
    )
    .unwrap();
    let mut writer = DraftWriter::new(database(&s.fixture));
    let client = writer.client();
    let aliases = [
        "personal-inbox:state-original-open",
        "personal-inbox:state-second-saved-filter",
    ];
    {
        let mut cache = Cache::open(&database(&s.fixture)).unwrap();
        let row = crate::storage::Record::new(
            "pr",
            "example/repo#7",
            REPO,
            json!({"state":"open","old_private_body":"unchanged-cache-body"}),
        );
        for alias in aliases {
            assert!(cache
                .commit_sync(
                    source.opening().scope(),
                    alias,
                    1,
                    std::slice::from_ref(&row),
                    true
                )
                .unwrap());
        }
        assert_eq!(
            cache
                .sync_frame(source.opening().scope(), aliases[0])
                .unwrap()
                .unwrap()
                .records[0],
            row
        );
    }
    let mut old_receipt: Option<(
        crate::providers::gh::StateReceiptV68,
        Arc<crate::providers::gh::OriginalPrStateV68>,
        CancellationToken,
    )> = None;
    let mut previous_draft: Option<(String, String)> = None;
    for attempt in 0..2 {
        let mut journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
        let revision = journal.state_action_revision_v68(&base).unwrap();
        assert_eq!(revision.is_some(), attempt == 1);
        let key = revision
            .as_ref()
            .map_or_else(|| base.key().to_owned(), |r| r.key().to_owned());
        saved_bodyless(&mut writer, &s, &key);
        let cancel = CancellationToken::default();
        let deadline = Instant::now() + Duration::from_secs(30);
        let provider = s
            .api
            .clone()
            .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
            .unwrap();
        let fence = client
            .capture_publication_v2(
                &database(&s.fixture),
                source.opening().scope(),
                &key,
                &cancel,
                deadline,
            )
            .unwrap();
        let receipt=source.with_state_phase_v68(StatePhaseRequestV68 {provider:&provider,database:&database(&s.fixture),cancel:&cancel,deadline,
            command:PrStateCommand::Close,postwrite:true},|mut phase| {
                let actual=Arc::clone(phase.actual());let local=phase.local(&database(&s.fixture))?;
                let seed=StateOriginSeedV68::from_producer(&source,&actual,&database(&s.fixture)).unwrap();
                let seed=match &revision {Some(revision)=>StateOriginSeedV68::from_revision_v68(seed,&base,revision).unwrap(),None=>seed};
                let write=seed.write(&BTreeMap::new()).unwrap();let mut captures=phase.take_captures()?;
                let permit=journal.begin_state_v68(local.target_v17(),&write,&seed).unwrap();
                let reference=permit.marker().publication_reference();
                let protected=crate::scoped_pr_collaboration_draft_v17::encode_draft_v17(&BTreeMap::new(),Some(&reference)).unwrap();
                let begun=local.generation_v17()+1;let claimed=begun+1;
                let ack=PrPlan::claim_persisted_v17(&mut journal,&permit,local.target_v17(),&write,|marker| {
                    let ack=client.save_publication_and_wait_v2(fence,protected.clone(),&cancel).map_err(|e|e.to_string())?;
                    local.validate_owned_boundary_v17(&database(&s.fixture),source.local_boundary(),marker,begun,PrWriteState::Pending,None).map_err(|e|e.to_string())?;
                    ack.check_current(&client,&database(&s.fixture),source.opening().scope(),&key,&cancel)?;Ok(ack)
                }).unwrap();
                let receipt=phase.publish(&provider,&mut captures,StatePublicationAckV68 {permit:&permit,ack:&ack,drafts:&client,key:&key},|| {
                    local.validate_owned_boundary_v17(&database(&s.fixture),source.local_boundary(),permit.marker(),claimed,PrWriteState::Pending,None)?;
                    journal.validate_owned_state_v68(&seed,&reference).map_err(|_|crate::scoped_pr_collaboration_ui_v17::state_v68::failure())?;
                    ack.check_current(&client,&database(&s.fixture),source.opening().scope(),&key,&cancel)
                        .map_err(|_|crate::scoped_pr_collaboration_ui_v17::state_v68::failure())
                }).map_err(|e|match e {crate::providers::gh::StatePublicationFailureV68::NotStarted(e)|crate::providers::gh::StatePublicationFailureV68::WriteMayHaveStarted(e)=>e})?;
                assert_eq!(captures.consumed_requests(),2,"Only actual receipt stage, not fresh-page completion evidence");
                let stable=all_sql(&s);
                assert!(journal.mark_confirmed(&permit,&receipt.effect()).is_err(),"Generic completion cannot half-confirm State");
                assert_eq!(all_sql(&s),stable);
                if let Some((old,old_actual,old_cancel))=&old_receipt {
                    assert!(!Arc::ptr_eq(old_actual,&actual));
                    assert_ne!(old.reference(),&reference);
                    // Keep the old genuine allocation/token LIVE, so this
                    // cannot pass merely because a Weak producer expired.
                    old.validate_commit_v68(old_cancel,deadline).unwrap();
                    assert!(journal.mark_state_confirmed_v68(&permit,&seed,old,claimed,old_cancel,deadline).is_err());
                    assert_eq!(all_sql(&s),stable);
                    assert_eq!(journal.blocking(local.target_v17()).unwrap().unwrap().publication_reference(),reference);
                }
                if repair_mode!=0 && attempt==1 {
                    let expected_generation=if repair_mode==2 {
                        let sql_before=all_sql(&s);
                        let fault=rusqlite::Connection::open(database(&s.fixture)).unwrap();
                        let schema=|db:&rusqlite::Connection|db.prepare("SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name").unwrap().query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<String>>(3)?))).unwrap().collect::<Result<Vec<_>,_>>().unwrap();
                        let original_schema=schema(&fault);
                        fault.execute_batch("CREATE TRIGGER state_test_refuse_disposition BEFORE INSERT ON pr_write_state_dispositions_v68 BEGIN SELECT RAISE(ABORT,'genuine local receipt persistence failure'); END;").unwrap();
                        assert!(journal.mark_state_confirmed_v68(&permit,&seed,&receipt,claimed,&cancel,deadline).is_err());
                        fault.execute_batch("DROP TRIGGER state_test_refuse_disposition;").unwrap();
                        assert_eq!(schema(&fault),original_schema);
                        assert_eq!(all_sql(&s),sql_before,"Receipt transaction failure rolls back every row, not just disposition");
                        journal.mark_uncertain(&permit).unwrap();
                        assert!(local.validate_owned_boundary_v17(&database(&s.fixture),source.local_boundary(),permit.marker(),claimed+1,PrWriteState::Uncertain,None).is_err(),"Ordinary owned validator is not widened");
                        claimed+1
                    } else {claimed};
                    let prepare=|| {
                        let repair=journal.prepare_original_state_cache_repair_v74(crate::pr_write_journal::state_origin_v68::StateRepairOriginalV74 {
                            source:&source,actual:&actual,seed:&seed,reference:&reference,local:&local,
                            generation:expected_generation,cancel:&cancel,deadline,
                        }).unwrap();
                        crate::storage::PreparedStateCacheRepairV74::prepare(repair,&cancel,deadline).unwrap()
                    };
                    let first=prepare();let stale_replay=prepare();
                    first.commit(&cancel,deadline).unwrap();
                    let fenced=all_sql(&s);
                    assert!(stale_replay.commit(&cancel,deadline).is_err(),"Original inactive source cannot be renewed by stale repair");
                    assert_eq!(all_sql(&s),fenced);
                    let db=rusqlite::Connection::open(database(&s.fixture)).unwrap();
                    let (ordinal,disposition,status):(i64,Option<String>,String)=db.query_row("SELECT attempt_id,raw_disposition_sha256,status FROM pr_write_state_filter_fences_v68",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
                    assert_eq!(ordinal,reference.attempt_id);assert!(disposition.is_none());assert_eq!(status,"unavailable");
                    assert_eq!(db.query_row("SELECT count(*) FROM pr_write_state_source_watermarks_v73",[],|r|r.get::<_,usize>(0)).unwrap(),0);
                    let cache=Cache::open(&database(&s.fixture)).unwrap();
                    for alias in aliases {
                        let error=cache.sync_frame(source.opening().scope(),alias).unwrap_err().to_string();
                        assert_eq!(error,"PR state/filter membership and counts unavailable; refresh this actual source explicitly","NULL remains unresolved, not malformed or fresh");
                    }
                    assert_eq!(all_sql(&s),fenced,"Pure cached refusal performs zero repair/hydration writes");
                    assert_eq!(cache.draft(source.opening().scope(),&key).unwrap().as_deref(),Some(protected.as_str()));
                    if repair_mode==2 {
                        let marker=journal.original_publication(local.target_v17(),&reference).unwrap().unwrap();
                        assert_eq!(marker.state,PrWriteState::Uncertain);assert!(marker.confirmed.is_none());
                        assert_eq!(journal.blocking(local.target_v17()).unwrap().unwrap().publication_reference(),reference);
                        return Ok((receipt,actual,cancel.clone()));
                    }
                }
                let effect=journal.mark_state_confirmed_v68(&permit,&seed,&receipt,claimed,&cancel,deadline).unwrap();
                let marker=journal.original_publication(local.target_v17(),&reference).unwrap().unwrap();
                assert_eq!(marker.state,PrWriteState::Confirmed);assert_eq!(marker.confirmed.as_ref(),Some(&effect));
                journal.state_origin_v68(local.target_v17(),&marker).unwrap();
                assert_eq!(Cache::open(&database(&s.fixture)).unwrap().draft(source.opening().scope(),&key).unwrap().as_deref(),Some(protected.as_str()));
                if let Some((old_key,old_raw))=&previous_draft {
                    assert_eq!(Cache::open(&database(&s.fixture)).unwrap().draft(source.opening().scope(),old_key).unwrap().as_deref(),Some(old_raw.as_str()));
                }
                if attempt==1 && repair_mode==1 {
                    let db=rusqlite::Connection::open(database(&s.fixture)).unwrap();
                    let (ordinal,hash,status):(i64,Option<String>,String)=db.query_row("SELECT attempt_id,raw_disposition_sha256,status FROM pr_write_state_filter_fences_v68",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
                    assert_eq!(ordinal,reference.attempt_id);assert_eq!(hash.unwrap().len(),64);assert_eq!(status,"unavailable");
                    assert_eq!(db.query_row("SELECT count(*) FROM pr_write_state_source_watermarks_v73",[],|r|r.get::<_,usize>(0)).unwrap(),0,"Typed receipt is not source hydration");
                }
                if attempt==1 && repair_mode==0 {
                    let terminal=journal.prepare_native_state_terminal_v68(crate::pr_write_journal::conversation_refresh_v19::StateTerminalV68 {
                        source:&source,actual:&actual,receipt:&receipt,permit:&permit,local:&local,
                        generation:claimed+1,effect:&effect,cancel:&cancel,
                    }).unwrap();
                    let prepared=crate::storage::conversation_refresh_v19::PreparedConversationOwnFenceV19::prepare(terminal,&cancel,deadline).unwrap();
                    let own=prepared.commit(&cancel,deadline).unwrap();
                    own.validate_before_read(&CancellationToken::default(),deadline).unwrap();
                    let stable=all_sql(&s);
                    let cache=Cache::open(&database(&s.fixture)).unwrap();
                    for alias in aliases {
                        assert!(cache.sync_frame(source.opening().scope(),alias).is_err(),"Replacing a shared global PR record cannot admit an old alias/count");
                    }
                    assert!(cache.sync_frame(source.opening().scope(),"personal-inbox:first-ever-unselected").is_err());
                    assert_eq!(all_sql(&s),stable,"Cached State admission neither reads GH nor writes/repairs a watermark");
                    assert_eq!(rusqlite::Connection::open(database(&s.fixture)).unwrap().query_row("SELECT count(*) FROM pr_write_state_source_watermarks_v73",[],|r|r.get::<_,usize>(0)).unwrap(),0);
                }
                Ok((receipt,actual,cancel.clone()))
            }).unwrap();
        previous_draft = Some((
            key.clone(),
            Cache::open(&database(&s.fixture))
                .unwrap()
                .draft(source.opening().scope(), &key)
                .unwrap()
                .unwrap(),
        ));
        old_receipt = Some(receipt);
    }
    let rows = fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), s.steps.len());
    let mutation_prefix = "7b227175657279223a226d75746174696f6e";
    assert_eq!(
        rows.iter()
            .filter(|r| r["args"][5] == "POST"
                && r["stdin_hex"]
                    .as_str()
                    .unwrap()
                    .starts_with(mutation_prefix))
            .count(),
        2
    );
    for (ordinal, (row, expected)) in rows.iter().zip(&s.steps).enumerate() {
        assert_eq!(row["step"], ordinal);
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(expected.args));
        assert_eq!(
            row["stdin_hex"],
            expected
                .stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
    }
    assert_eq!(git(&s.fixture, &["rev-parse", "HEAD"]), s.git[0]);
    for (path, wanted) in [
        (".git/index", &s.git[1]),
        (".git/config", &s.git[2]),
        ("kept", &s.git[3]),
    ] {
        assert_eq!(&fs::read(s.fixture.repo().join(path)).unwrap(), wanted);
    }
}
