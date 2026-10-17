//! Actual closed producers/strict transport/held owned final-UID child. These
//! are NEW raw-row contracts; old Review recovery tests remain regressions.
use super::*;
#[test]
fn pending_review_exact_ack_v17_actual_held_final_uid_receipt_created_target_or_origin_drift_never_releases_blocker(
) {
    for drift in ["receipt", "created", "target", "origin"] {
        let real = real_with_ack_delay_v17(false, true, true, 6000);
        let prepared = inspect(&real, request(&real));
        let path = database(&real.setup.fixture);
        let before = commands(&real.setup.fixture).len();
        let generation = PrWriteJournal::open(&path)
            .unwrap()
            .source_generation(&real.target)
            .unwrap();
        std::thread::scope(|threads| {
            let work = threads.spawn(|| {
                recovery::acknowledge(
                    &prepared,
                    &real.api,
                    &path,
                    &Default::default(),
                    Instant::now() + Duration::from_secs(30),
                )
            });
            let index = before + summary_steps(&real.setup.h).len() - 1;
            let pid = wait_for_original_call(&real.setup.fixture, index);
            let held = HeldOriginalChild::stop(pid);
            let sql = rusqlite::Connection::open_with_flags(
                &path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
            )
            .unwrap();
            let changed = match drift {
                "receipt" => sql.execute("UPDATE pr_write_attempts SET confirmed_json=?1 WHERE attempt_id=?2", rusqlite::params!["{}",real.reference.attempt_id]).unwrap(),
                "created" => sql.execute("UPDATE pr_write_attempts SET created_at=created_at+1 WHERE attempt_id=?1", [real.reference.attempt_id]).unwrap(),
                "target" => sql.execute("UPDATE pr_write_attempts SET repository='foreign/replaced' WHERE attempt_id=?1", [real.reference.attempt_id]).unwrap(),
                "origin" => sql.execute("INSERT INTO pr_write_review_operation_origins_v3(attempt_id,repository_id,repository_node,pr_id,origin_json) VALUES(?1,1,'foreign-origin-node',2,'null')", [real.reference.attempt_id]).unwrap(),
                _ => unreachable!(),
            };
            assert_eq!(changed, 1);
            let raw = sql_state(&real.setup.fixture);
            held.resume();
            assert!(
                work.join().unwrap().is_err(),
                "held final UID drift {drift} must not ACK"
            );
            assert_eq!(
                sql_state(&real.setup.fixture),
                raw,
                "no private draft/cache effects after refusal"
            );
        });
        let sql = rusqlite::Connection::open_with_flags(
            &path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        assert_eq!(
            sql.query_row(
                "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
                [real.reference.attempt_id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "uncertain"
        );
        assert_eq!(
            PrWriteJournal::open(&path)
                .unwrap()
                .source_generation(&real.target)
                .unwrap(),
            generation
        );
        assert_eq!(
            Cache::open(&path)
                .unwrap()
                .draft(real.target.scope(), &real.key)
                .unwrap()
                .as_deref(),
            Some(real.encoded.as_str())
        );
        unchanged_git_and_exact_script(&real.setup);
    }
}
#[test]
fn pending_review_exact_ack_v17_unchanged_actual_review_releases_only_state_once_preserving_raw_target_and_body(
) {
    let real = real(false, true, true);
    let prepared = inspect(&real, request(&real));
    let path = database(&real.setup.fixture);
    let raw = |sql: &rusqlite::Connection| {
        sql.query_row("SELECT host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,created_at,confirmed_json FROM pr_write_attempts WHERE attempt_id=?1", [real.reference.attempt_id], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,u64>(4)?,r.get::<_,String>(5)?,r.get::<_,String>(6)?,r.get::<_,String>(7)?,r.get::<_,String>(8)?,r.get::<_,u64>(9)?,r.get::<_,Option<String>>(10)?))).unwrap()
    };
    let connection =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let original = raw(&connection);
    let generation = PrWriteJournal::open(&path)
        .unwrap()
        .source_generation(&real.target)
        .unwrap();
    let result = recovery::acknowledge(
        &prepared,
        &real.api,
        &path,
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert_eq!(result.changed, 1);
    assert_eq!(result.reference, real.reference);
    assert_eq!(raw(&connection), original);
    assert_eq!(
        PrWriteJournal::open(&path)
            .unwrap()
            .source_generation(&real.target)
            .unwrap(),
        generation + 1
    );
    assert_kept(&real, PrWriteState::Acknowledged);
}
