//! Same actual Return factory and Background queue; no constructed origin,
//! publication/receipt/result or source. Existing fields fault only transport.
use super::*;
use crate::personal_pr_entry_v13::tests::collaboration_v17::protocol::publication::refresh_v19 as fixture;
use crate::{app::Action, forms::FormAction};
use std::sync::mpsc;

#[test]
fn conversation_refresh_v19_actual_full_disconnected_none_queue_refuse_before_form_slot_or_sql_effects(
) {
    for mode in 0..3 {
        let (mut s, _finished, writer) = fixture::background(fixture::unavailable_tail());
        fixture::paint(&mut s.app);
        s.app
            .dispatch(Action::Form(FormAction::RefreshConversationV19));
        native::prepare_requested(&mut s.app);
        let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
        let mut bg = Background::new();
        let (tx, rx) = mpsc::sync_channel(0);
        let held = match mode {
            0 => {
                bg.requests = Some(tx);
                Some(rx)
            }
            1 => {
                drop(rx);
                bg.requests = Some(tx);
                None
            }
            _ => {
                bg.requests = None;
                None
            }
        };
        let sql = fixture::exact_sql(&s);
        let generation = bg.generation;
        let status = s.app.status.clone();
        let loading = s.app.loading;
        let form = s.app.form.as_ref().unwrap().presentation_v4().0.clone();
        let input = s.app.form.as_ref().unwrap().presentation_v4().1;
        assert!(!bg.submit_scoped_pr_collaboration_v17(
            &mut s.app,
            work,
            s.api.clone(),
            s.fixture.root.join("cache.sqlite3"),
            writer.client()
        ));
        assert_eq!(bg.generation, generation);
        assert!(bg.current.is_empty());
        assert!(bg.current_collaboration_v17.is_none());
        assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
        assert_eq!(s.app.loading, loading);
        assert_eq!(s.app.status, status);
        assert!(Arc::ptr_eq(
            s.app.form.as_ref().unwrap().presentation_v4().0,
            &form
        ));
        assert_eq!(s.app.form.as_ref().unwrap().presentation_v4().1, input);
        assert!(!s.app.form.as_ref().unwrap().submitting);
        assert!(s.app.scoped_pr_collaboration_return_v19.is_some());
        assert_eq!(fixture::exact_sql(&s), sql);
        drop(held);
    }
}

#[test]
fn conversation_refresh_v19_actual_held_read_generic_replacement_success_error_panic_and_newer_presentation_never_retire_successor(
) {
    use crate::personal_pr_entry_v13::tests as personal;
    use crate::test_support::GhStep;
    use serde_json::json;
    for mode in 0..6 {
        let mut tail = fixture::unavailable_tail();
        let mut read =
            crate::personal_pr_entry_v13::tests::collaboration_v17::refresh_v19::refresh_phase();
        read[3].delay_ms = 2000;
        read.truncate(4);
        tail.extend(read);
        let (mut s, mut bg, writer) = fixture::background(tail);
        fixture::paint(&mut s.app);
        s.app
            .dispatch(Action::Form(FormAction::RefreshConversationV19));
        native::prepare_requested(&mut s.app);
        let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
        assert!(bg.submit_scoped_pr_collaboration_v17(
            &mut s.app,
            work,
            s.api.clone(),
            s.fixture.root.join("cache.sqlite3"),
            writer.client()
        ));
        let old = s.app.scoped_pr_collaboration_owner_v17.clone().unwrap();
        let stopped = fixture::StoppedV19::final_uid(&s);
        let (send, receive) = mpsc::sync_channel(1);
        let (entered, ready) = mpsc::sync_channel(1);
        assert!(bg.submit(&mut s.app, View::Inbox, move |cancel| {
            entered.send(cancel).unwrap();
            receive
                .recv_timeout(std::time::Duration::from_secs(8))
                .unwrap();
            match mode {
                1 => Err("ACTUAL_V19_NEWER_ERROR".into()),
                2 => panic!("ACTUAL_V19_CAUGHT_NEWER_PANIC"),
                _ => Ok(PreparedView {
                    status: "ACTUAL_V19_NEWER_SUCCESS".into(),
                    ..Default::default()
                }),
            }
        }));
        let next_token = ready
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let generation = bg.current[&View::Inbox.index()].0;
        let mut independent = None;
        match mode {
            3 => {
                let mut fresh = personal::setup(Vec::<GhStep>::new());
                s.app = std::mem::take(&mut fresh.app);
                independent = Some(fresh);
            }
            4 => {
                let workspace = crate::workspace::Workspace::load(
                    &s.fixture.repo(),
                    crate::workspace::RepositoryMode::Working,
                    "HEAD",
                    None,
                )
                .unwrap()
                .unwrap();
                s.app.form = Some(
                    crate::forms::Form::local(
                        crate::local_actions::LocalCommand::CreateBranch,
                        crate::local_actions::LocalContext::from_workspace(&workspace, None, None),
                    )
                    .unwrap(),
                );
                s.app.overlay = Some(crate::app::Overlay::Form);
                s.app.view = View::Repository;
            }
            5 => {
                s.app.overlay = Some(crate::app::Overlay::Notice);
                s.app.notice = "NEWER_NOTICE_CONTROLS".into();
                s.app.hits = vec![crate::app::HitRegion {
                    rect: ratatui::layout::Rect::new(3, 4, 12, 1),
                    action: Action::Close,
                }];
            }
            _ => {}
        }
        s.app.status = "NEWER_SLOT_STATUS".into();
        let status = s.app.status.clone();
        let loading = s.app.loading;
        let overlay = s.app.overlay;
        let hits = s
            .app
            .hits
            .iter()
            .map(|h| (h.rect, h.action.clone()))
            .collect::<Vec<_>>();
        let newer_form = if mode == 4 {
            Some(s.app.form.as_ref().unwrap().presentation_v4().0.clone())
        } else {
            None
        };
        let page = s
            .app
            .github
            .as_ref()
            .and_then(|g| g.pr.as_ref())
            .map(|p| p.details.summary.pr.body.as_ptr());
        bg.retire_stale_scoped_pr_collaboration_v17(&mut s.app);
        drop(stopped);
        assert!(old.cancellation().is_cancelled());
        assert_eq!(bg.current[&View::Inbox.index()].0, generation);
        assert!(bg.current[&View::Inbox.index()]
            .1
            .same_instance(&next_token));
        assert!(!next_token.is_cancelled());
        assert_eq!(s.app.loading, loading);
        assert_eq!(s.app.status, status);
        if mode == 4 {
            assert!(Arc::ptr_eq(
                s.app.form.as_ref().unwrap().presentation_v4().0,
                newer_form.as_ref().unwrap()
            ));
        }
        if mode == 5 {
            assert_eq!(s.app.overlay, overlay);
            assert_eq!(s.app.notice, "NEWER_NOTICE_CONTROLS");
            assert_eq!(
                s.app
                    .hits
                    .iter()
                    .map(|h| (h.rect, h.action.clone()))
                    .collect::<Vec<_>>(),
                hits
            );
        }
        assert_eq!(
            s.app.github.as_ref().and_then(|g| g.pr.as_ref()).map(|p| p
                .details
                .summary
                .pr
                .body
                .as_ptr()),
            page
        );
        let sql = fixture::exact_sql(&s);
        let mut old_result = bg
            .completions
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(old_result
            .remote_scope_owner
            .as_ref()
            .is_some_and(|r| r.collaboration_v17.is_some()));
        assert_eq!(
            bg.poll_scoped_pr_collaboration_v17(&mut s.app, &mut old_result),
            Some(false)
        );
        assert_eq!(fixture::exact_sql(&s), sql);
        assert_eq!(s.app.status, status);
        assert_eq!(s.app.loading, loading);
        assert_eq!(bg.current[&View::Inbox.index()].0, generation);
        assert!(!next_token.is_cancelled());
        send.send(()).unwrap();
        let completion = bg
            .completions
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert_eq!(completion.generation, generation);
        assert_eq!(completion.result.is_err(), matches!(mode, 1 | 2));
        assert!(completion.remote_scope_owner.is_none());
        assert!(bg.request_cancel(View::Inbox));
        drop(independent);
        let rows = std::fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl"))
            .unwrap()
            .lines()
            .map(|r| serde_json::from_str::<serde_json::Value>(r).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), s.steps.len());
        for (actual, expected) in rows.iter().zip(&s.steps) {
            assert_eq!(actual["matched"], true);
            assert_eq!(actual["args"], json!(expected.args));
            assert_eq!(
                actual["stdin_hex"],
                expected
                    .stdin
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            );
        }
    }
}
