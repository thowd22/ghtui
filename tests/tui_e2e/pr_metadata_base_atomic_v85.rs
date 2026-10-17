//! The actual Base publisher shares the same atomic projection/draft worker.
use super::*;
use refreshed_v19::atomic_v85::{full_sql_v85, privacy_oracle_v85, protected_sql_v85, HeldUidV85};
fn run(mouse: bool) {
    for personal in [false, true] {
        for uid in [42, 43] {
            let mut steps = initial_steps(personal);
            let initial = steps.len();
            steps.extend(catalogue(json!(true)));
            let opened = steps.len();
            steps.extend(selected());
            let previewed = steps.len();
            let mut writes = paid();
            let last = writes.last_mut().unwrap();
            *last = get("user", json!({"id":uid,"login":"octocat"}));
            last.delay_ms = 2000;
            steps.extend(writes);
            let (f, mut s, git, config, stable) = begin(&steps, personal, mouse, initial);
            enter_base(&mut s, &f, mouse, opened);
            choose_member(&mut s, mouse, "next");
            preview(&mut s, &f, mouse, previewed);
            control(
                &mut s,
                mouse,
                &title(),
                "[Confirm action]",
                PREVIEW_CONTROLS,
            );
            let mut held = HeldUidV85::capture(&mut s, &f, &steps);
            state(&f, "confirmed", true);
            let db = raw(&f);
            let (key, body): (String, String) = db
                .query_row(
                    "SELECT resource,body FROM drafts WHERE resource LIKE 'metadata-base-v32/%'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            let before = full_sql_v85(&f);
            let privacy =
                (uid == 43).then(|| privacy_oracle_v85(&f, &["review", "inline_comment"]));
            assert!(Cache::open(&database(&f))
                .unwrap()
                .sync_frame(
                    &Scope::github(HOST, Some(42)).unwrap(),
                    "pr-write-projection:GENUINE_CANONICAL_PR_NODE_1007"
                )
                .unwrap()
                .is_none());
            held.release();
            if uid == 42 {
                refreshed_v19::wait_fresh_page_v19(&mut s);
                state(&f, "confirmed", false);
                let parent:Value = serde_json::from_str(&db.query_row("SELECT payload FROM records WHERE kind='pr-details' AND id='GENUINE_CANONICAL_PR_NODE_1007'",[],|r|r.get::<_,String>(0)).unwrap()).unwrap();
                assert_eq!(parent["summary"]["base_ref"], "next");
                assert_eq!(parent["summary"]["base_sha"], TIP);
                assert_eq!(parent["summary"]["head_sha"], "a".repeat(40));
                let attempt: i64 = db
                    .query_row("SELECT attempt_id FROM pr_write_attempts", [], |r| r.get(0))
                    .unwrap();
                let mut q = db.prepare("SELECT kind,mutation_ordinal FROM sync_mutation_fences WHERE resource='GENUINE_CANONICAL_PR_NODE_1007' ORDER BY kind").unwrap();
                assert_eq!(
                    q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
                        .unwrap()
                        .map(Result::unwrap)
                        .collect::<Vec<_>>(),
                    vec![
                        ("inline_comment".into(), attempt),
                        ("review".into(), attempt)
                    ]
                );
                assert_ne!(full_sql_v85(&f), before);
            } else {
                s.wait_for_cursorless_condition(
                    |frame| {
                        crate::support::main_footer_visible(frame)
                            && frame
                                .contents()
                                .contains("Confirmed; visibility unavailable; no resend")
                            && !frame.contents().contains(DETAIL_BODY)
                            && !frame.contents().contains("[Confirm action]")
                            && !frame.contents().contains("[r Refresh PR]")
                    },
                    "Base final UID loss preserves exact protected SQL; no B1 page is admitted",
                    Duration::from_secs(8),
                );
                assert_eq!(
                    protected_sql_v85(&full_sql_v85(&f)),
                    protected_sql_v85(&before)
                );
                assert_eq!(
                    full_sql_v85(&f),
                    privacy.unwrap(),
                    "Only exact Base two-category privacy fencing may change derived SQL"
                );
                assert_eq!(
                    db.query_row("SELECT body FROM drafts WHERE resource=?1", [&key], |r| {
                        r.get::<_, String>(0)
                    })
                    .unwrap(),
                    body
                );
                state(&f, "confirmed", true);
                assert!(Cache::open(&database(&f))
                    .unwrap()
                    .sync_frame(
                        &Scope::github(HOST, Some(42)).unwrap(),
                        "pr-write-projection:GENUINE_CANONICAL_PR_NODE_1007"
                    )
                    .unwrap()
                    .is_none());
            }
            held.reaped(&mut s);
            s.capture_completed_terminal_v1("base-atomic-v85-final-uid-exact-draft-projection");
            finish(s, &f, mouse, &steps, &git, &config, &stable);
        }
    }
}
#[test]
fn pr_metadata_base_v32_atomic_keyboard_both_originals_held_final_uid_success_and_loss_exact_sql() {
    run(false)
}
#[test]
fn pr_metadata_base_v32_atomic_mouse_both_originals_held_final_uid_success_and_loss_exact_sql() {
    run(true)
}
