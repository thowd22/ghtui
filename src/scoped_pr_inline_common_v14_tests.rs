//! C14 contracts use actual retained source factories, installed Git, strict gh
//! and actual existing queue/SQL. Direct execute contracts do not prove native
//! prequeue ordering; the separate actual Background contract does.
use super::*;

#[test]
fn inline_comments_common_v14_prepaid_allowance_refuses_small_original_total_before_post() {
    let mut s = inline_setup(|h, steps| {
        steps.extend(image_steps(h));
        steps.extend(principal(42, "inline-publisher"));
    });
    let preview = prepared(&mut s);
    let writer = DraftWriter::new(database(&s.fixture));
    let mut access = crate::providers::gh::RepositoryCatalogueAccessLimits::default();
    access.transport.max_response_bytes = 2 * 1024 * 1024;
    let before = commands(&s.fixture).len();
    let error = preview
        .execute(
            &provider(&s.fixture),
            &database(&s.fixture),
            &Default::default(),
            &access,
            &writer.client(),
            true,
            None,
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap_err();
    assert!(error.hide_private);
    assert_eq!(commands(&s.fixture).len() - before, 2);
    assert_eq!(
        sql(&s.fixture)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| row
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
    unchanged_git_and_exact_script(&s);
}

#[test]
fn inline_comments_common_v14_original_narrow_capture_cannot_be_widened_for_confirm() {
    let mut s = inline_setup(|h, steps| {
        steps.extend(image_steps(h));
        steps.extend(confirm_steps(h).into_iter().take(3));
    });
    let preview = prepared(&mut s);
    let writer = DraftWriter::new(database(&s.fixture));
    let deadline = Instant::now() + Duration::from_secs(30);
    let api = provider(&s.fixture)
        .with_scoped_read_window(deadline, 128)
        .unwrap();
    let before = commands(&s.fixture).len();
    assert!(preview
        .execute(
            &api,
            &database(&s.fixture),
            &Default::default(),
            &Default::default(),
            &writer.client(),
            true,
            None,
            &Default::default(),
            deadline
        )
        .is_err());
    assert_eq!(commands(&s.fixture).len() - before, 3);
    assert_eq!(
        sql(&s.fixture)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| row
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
    unchanged_git_and_exact_script(&s);
}

fn external_replacement(after_receipt: bool) {
    let phase = if after_receipt { 15 } else { 7 };
    let mut s = inline_setup(|h, steps| {
        steps.extend(image_steps(h));
        let mut confirm = confirm_steps(h);
        confirm[phase].delay_ms = 6_000;
        confirm.truncate(if after_receipt { 16 } else { 9 });
        steps.extend(confirm);
    });
    let preview = prepared(&mut s);
    let writer = DraftWriter::new(database(&s.fixture));
    let client = writer.client();
    let before = commands(&s.fixture).len();
    let replacement = "INDEPENDENT_NEW_EXTERNAL_BODY_AND_REFERENCE";
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            preview.execute(
                &provider(&s.fixture),
                &database(&s.fixture),
                &Default::default(),
                &Default::default(),
                &client,
                true,
                None,
                &Default::default(),
                Instant::now() + Duration::from_secs(30),
            )
        });
        let pid = wait_for_original_call(&s.fixture, before + phase);
        let held = HeldOriginalChild::stop(pid);
        let encoded: String = sql(&s.fixture)
            .query_row(
                "SELECT body FROM drafts WHERE resource=?1",
                [preview.draft_key()],
                |row| row.get(0),
            )
            .unwrap();
        let original: Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(original["values"]["body"], INLINE_BODY);
        assert_eq!(
            rusqlite::Connection::open_with_flags(
                database(&s.fixture),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
            )
            .unwrap()
            .execute(
                "UPDATE drafts SET body=?1 WHERE resource=?2",
                rusqlite::params![replacement, preview.draft_key()]
            )
            .unwrap(),
            1
        );
        held.resume();
        let result = worker.join().unwrap().unwrap();
        assert!(result.hide_private());
        assert!(result.draft_retained());
        assert_eq!(
            result.state(),
            if after_receipt {
                ScopedInlinePublicationStateV1::ConfirmedButSourceUnavailable
            } else {
                ScopedInlinePublicationStateV1::NotStarted
            }
        );
        assert_eq!(result.confirmed_comment_id(), after_receipt.then_some(901));
        let marker = publication_marker(&s, &preview, result.reference());
        assert_eq!(
            marker.state,
            if after_receipt {
                crate::pr_write_journal::PrWriteState::Confirmed
            } else {
                crate::pr_write_journal::PrWriteState::Pending
            }
        );
        assert_eq!(
            sql(&s.fixture)
                .query_row(
                    "SELECT body FROM drafts WHERE resource=?1",
                    [preview.draft_key()],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            replacement
        );
    });
    assert_eq!(
        commands(&s.fixture)
            .iter()
            .filter(|row| row["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg == "POST"))
            .count(),
        usize::from(after_receipt)
    );
    unchanged_git_and_exact_script(&s);
}

#[test]
fn inline_comments_common_v14_external_sql_replacement_before_post_preserves_pending_and_new_body()
{
    external_replacement(false);
}
#[test]
fn inline_comments_common_v14_external_sql_replacement_after_receipt_preserves_confirmed_and_new_body(
) {
    external_replacement(true);
}

#[test]
fn inline_comments_common_v14_native_expired_original_cannot_cancel_independent_inbox_slot() {
    let mut s = inline_setup(|_, _| {});
    let (mut app, work) = original_open(&mut s);
    let source = app.remote_scope_source.as_ref().unwrap().clone();
    let navigation = app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_navigation
        .as_ref()
        .unwrap()
        .clone();
    let writer = DraftWriter::new(database(&s.fixture));
    let mut background = Background::new();
    let (started, waiting) = std::sync::mpsc::sync_channel(1);
    let (release, blocked) = std::sync::mpsc::sync_channel(1);
    assert!(background.submit(&mut app, View::Inbox, move |cancel| {
        started.send(cancel.clone()).unwrap();
        blocked.recv().unwrap();
        Ok(crate::runtime::PreparedView {
            status: "INDEPENDENT_CURRENT_RESULT".into(),
            ..Default::default()
        })
    }));
    let token = waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    let generation = channel::generation(&background);
    app.status = "INDEPENDENT_CURRENT_STATUS".into();
    let api = provider(&s.fixture)
        .with_scoped_read_window(Instant::now(), 1024 * 1024)
        .unwrap();
    assert!(!background.submit_scoped_inline_v1(
        &mut app,
        work,
        api,
        database(&s.fixture),
        writer.client()
    ));
    assert_eq!(channel::generation(&background), generation);
    assert!(!token.is_cancelled());
    assert!(app.loading[View::Inbox.index()]);
    assert_eq!(app.status, "INDEPENDENT_CURRENT_STATUS");
    assert!(app.scoped_inline_owner_v1.is_none());
    assert!(Arc::ptr_eq(
        app.remote_scope_source.as_ref().unwrap(),
        &source
    ));
    assert!(Arc::ptr_eq(
        app.github
            .as_ref()
            .unwrap()
            .scoped_pr_navigation
            .as_ref()
            .unwrap(),
        &navigation
    ));
    release.send(()).unwrap();
    completed(&mut background, &mut app);
    assert_eq!(app.status, "INDEPENDENT_CURRENT_RESULT");
    unchanged_git_and_exact_script(&s);
}

#[test]
fn inline_comments_common_v14_actual_invoked_orphan_retires_only_old_presentation_and_preserves_successor(
) {
    // The normal pending Form admits only Esc. This actual Background race is
    // deliberately a CODE contract, not a fabricated user-reachable PTY path.
    for next in 0..6 {
        let mut s = inline_setup(|h, steps| {
            steps.extend(source_fence_steps(h, true));
            steps.extend(image_steps(h));
            if next == 1 {
                steps.extend(image_steps(h));
            }
            if next == 3 {
                steps.extend(source_fence_steps(h, true));
            }
            let mut confirm = confirm_steps(h);
            confirm[12].delay_ms = 30_000;
            confirm.truncate(13);
            steps.extend(confirm);
            steps.extend(principal(42, "actual-successor-observer"));
        });
        let mut form = super::super::inline_ui::form(&mut s);
        let request = ui::request(&form).unwrap();
        let preview = super::super::inline_ui::preview(&s, &request, request.body().clone());
        assert!(ui::accept_preview(&mut form, &request, preview));
        let mut app = app(&s);
        app.overlay = Some(Overlay::Form);
        app.form = Some(form);
        let fresh = if next == 1 {
            let files = app
                .github
                .as_ref()
                .unwrap()
                .scoped_pr_resource
                .clone()
                .unwrap();
            Some((
                files,
                read(&mut s, &Default::default(), &Default::default()).unwrap(),
            ))
        } else {
            None
        };
        let mut independent = if next == 2 {
            let mut other = inline_setup(|_, _| {});
            let (other_app, _) = original_open(&mut other);
            Some((other, other_app))
        } else {
            None
        };
        let newer_form = if next == 3 {
            Some(
                ui::form_from_draft(
                    ScopedInlineDraftV1::open(
                        request.intent().clone(),
                        &provider(&s.fixture),
                        &database(&s.fixture),
                        &Default::default(),
                        &Default::default(),
                        &Default::default(),
                        Instant::now() + Duration::from_secs(30),
                    )
                    .unwrap(),
                )
                .unwrap(),
            )
        } else {
            None
        };
        app.form.as_mut().unwrap().submit_requested = true;
        native::capture_form(&mut app);
        let work = app.scoped_inline_work_v1.take().unwrap();
        let writer = DraftWriter::new(database(&s.fixture));
        let mut bg = Background::new();
        submit(&s, &mut app, &mut bg, work, &writer);
        let old = app.scoped_inline_owner_v1.clone().unwrap();
        let readback = s
            .steps
            .iter()
            .position(|step| step.delay_ms == 30_000)
            .unwrap();
        let pid = wait_for_original_call(&s.fixture, readback);
        let held = HeldOriginalChild::stop(pid);
        let calls = commands(&s.fixture);
        assert!(calls[readback - 1]["args"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "POST"));
        let original: String = sql(&s.fixture)
            .query_row(
                "SELECT body FROM drafts WHERE resource LIKE 'inline/%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let original: Value = serde_json::from_str(&original).unwrap();
        assert_eq!(original["values"]["body"], INLINE_BODY);
        if let Some((files, result)) = fresh {
            let github = app.github.as_mut().unwrap();
            assert!(Arc::ptr_eq(
                github.scoped_pr_resource.as_ref().unwrap(),
                &files
            ));
            assert!(Arc::ptr_eq(
                github.scoped_pr_navigation.as_ref().unwrap(),
                &result.requested_navigation
            ));
            let row = (0..github.pr.as_ref().unwrap().rows.len())
                .find(|row| {
                    matches!(
                        files.row(*row),
                        Some(
                            crate::scoped_pr_observed_tabs_v1::resources::PrResourceRowV1::FileList(
                                0
                            )
                        )
                    )
                })
                .unwrap();
            drop(images::take_image_input(github, row).unwrap());
            let (viewer, limited) = images::install_images(&mut app, result).unwrap();
            assert!(!limited);
            app.pr_file_images = viewer;
        }
        if let Some((_, other)) = &mut independent {
            app.github = other.github.take();
            app.remote_scope_source = other.remote_scope_source.take();
            app.remote_scope = other.remote_scope.take();
            app.pr_file_images = other.pr_file_images.take();
        }
        let current_request = if let Some(form) = newer_form {
            app.form = Some(form);
            Some(ui::request(app.form.as_ref().unwrap()).unwrap())
        } else {
            None
        };
        let source = app.remote_scope_source.clone().unwrap();
        let scope = app.remote_scope.clone().unwrap();
        let nav = app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_navigation
            .clone()
            .unwrap();
        let files = app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_resource
            .clone()
            .unwrap();
        let api = provider(&s.fixture);
        let (started, ready) = std::sync::mpsc::sync_channel(1);
        let (release, wait) = std::sync::mpsc::sync_channel(1);
        assert!(bg.submit_scoped_pr_tab(
            &mut app,
            scope.clone(),
            source.clone(),
            nav.clone(),
            move |cancel| {
                started.send(cancel.clone()).unwrap();
                wait.recv_timeout(Duration::from_secs(8)).unwrap();
                let identity = api
                    .identity(&cancel)
                    .map_err(|_| "actual successor principal unavailable".to_owned())?;
                Ok(crate::runtime::PreparedView {
                    status: "ACTUAL_ORIGINAL_NAV_SUCCESSOR".into(),
                    verified_identity: Some(identity),
                    ..Default::default()
                })
            }
        ));
        let generation = channel::generation(&bg);
        app.status = "SUCCESSOR_STATUS".into();
        let loading = app.loading;
        let token = ready.recv_timeout(Duration::from_secs(8)).unwrap();
        assert!(!token.is_cancelled());
        let overlay_witness = if next >= 4 {
            let before = commands(&s.fixture).len();
            if next == 4 {
                app.dispatch(crate::app::Action::Explain(
                    "INDEPENDENT_NEW_NOTICE_CONTROL_WITNESS".into(),
                ));
                assert_eq!(app.overlay, Some(Overlay::Notice));
            } else {
                app.dispatch(crate::app::Action::Palette);
                assert_eq!(app.overlay, Some(Overlay::Palette));
                assert!(app.command_palette.is_some());
            }
            assert!(old
                .work()
                .request()
                .unwrap()
                .owns_presentation_v14(app.form.as_ref().unwrap()));
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
            terminal
                .draw(|frame| crate::ui::draw(frame, &mut app))
                .unwrap();
            let text = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(text.contains(if next == 4 {
                "INDEPENDENT_NEW_NOTICE_CONTROL_WITNESS"
            } else {
                "Commands"
            }));
            assert!(
                !app.hits.is_empty(),
                "Genuine newer overlay must paint controls"
            );
            assert!(app.hits.iter().any(|hit| if next == 4 {
                hit.action == crate::app::Action::Close
            } else {
                matches!(hit.action, crate::app::Action::PaletteInput(_))
            }));
            assert_eq!(commands(&s.fixture).len(), before);
            Some((
                app.overlay,
                app.hits
                    .iter()
                    .map(|hit| (hit.rect, hit.action.clone()))
                    .collect::<Vec<_>>(),
                app.notice.clone(),
                app.command_source_epoch,
                app.command_palette_context,
                app.command_palette.as_ref().map(|palette| {
                    (
                        palette.selected_id().map(str::to_owned),
                        palette.query().to_owned(),
                        palette.visible_indices().to_vec(),
                    )
                }),
            ))
        } else {
            None
        };
        bg.retire_stale_scoped_inline_v1(&mut app);
        assert_eq!(channel::generation(&bg), generation);
        assert_eq!(app.loading, loading);
        assert_eq!(app.status, "SUCCESSOR_STATUS");
        assert!(old.cancellation().is_cancelled());
        assert!(app.scoped_inline_owner_v1.is_none());
        assert!(Arc::ptr_eq(
            app.remote_scope_source.as_ref().unwrap(),
            &source
        ));
        assert!(Arc::ptr_eq(app.remote_scope.as_ref().unwrap(), &scope));
        assert!(Arc::ptr_eq(
            app.github
                .as_ref()
                .unwrap()
                .scoped_pr_navigation
                .as_ref()
                .unwrap(),
            &nav
        ));
        if next == 0 || next >= 4 {
            assert!(app.github.as_ref().unwrap().pr.is_none());
            assert!(app.github.as_ref().unwrap().scoped_pr_resource.is_none());
            assert!(app.form.is_none());
            assert!(app.pr_file_images.is_none());
            assert!(app.pr_native_diff_v1.is_none());
        } else {
            assert!(app.github.as_ref().unwrap().pr.is_some());
            assert!(Arc::ptr_eq(
                app.github
                    .as_ref()
                    .unwrap()
                    .scoped_pr_resource
                    .as_ref()
                    .unwrap(),
                &files
            ));
            if let Some(request) = current_request {
                assert!(request.matches_form(app.form.as_ref().unwrap()));
            } else {
                assert!(app.form.is_none());
            }
        }
        if let Some((overlay, hits, notice, epoch, context, palette)) = overlay_witness {
            assert_eq!(app.overlay, overlay);
            assert_eq!(
                app.hits
                    .iter()
                    .map(|hit| (hit.rect, hit.action.clone()))
                    .collect::<Vec<_>>(),
                hits,
                "Old Confirm must not erase controls belonging to a newer overlay"
            );
            assert_eq!(app.notice, notice);
            assert_eq!(app.command_source_epoch, epoch);
            assert_eq!(app.command_palette_context, context);
            assert_eq!(
                app.command_palette.as_ref().map(|palette| {
                    (
                        palette.selected_id().map(str::to_owned),
                        palette.query().to_owned(),
                        palette.visible_indices().to_vec(),
                    )
                }),
                palette
            );
            assert_eq!(app.command_palette.is_some(), next == 5);
            assert!(app.form_dismissed);
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
            terminal
                .draw(|frame| crate::ui::draw(frame, &mut app))
                .unwrap();
            let text = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(!text.contains(INLINE_BODY));
            assert!(text.contains(if next == 4 {
                "INDEPENDENT_NEW_NOTICE_CONTROL_WITNESS"
            } else {
                "Commands"
            }));
            assert_eq!(
                app.hits
                    .iter()
                    .map(|hit| (hit.rect, hit.action.clone()))
                    .collect::<Vec<_>>(),
                hits,
                "Newer overlay controls must survive the next actual paint too"
            );
            assert_eq!(channel::generation(&bg), generation);
            assert_eq!(app.loading, loading);
            assert_eq!(app.status, "SUCCESSOR_STATUS");
        }
        assert!(!token.is_cancelled());
        release.send(()).unwrap();
        completed(&mut bg, &mut app);
        assert_eq!(app.status, "ACTUAL_ORIGINAL_NAV_SUCCESSOR");
        assert!(!token.is_cancelled());
        drop(held);
        let deadline = Instant::now() + Duration::from_secs(8);
        while original_process_identity::process_identity(pid)
            .unwrap()
            .is_some()
        {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        let marker = PrWriteJournal::open(&database(&s.fixture))
            .unwrap()
            .original_publication(
                &crate::pr_write_journal::PrWriteTarget::for_source(
                    request.intent().scope(),
                    REPO,
                    "GENUINE_PR_NODE_1007",
                    7,
                )
                .unwrap(),
                &serde_json::from_value(original["publication"].clone()).unwrap(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            marker.state,
            crate::pr_write_journal::PrWriteState::Uncertain
        );
        assert_eq!(
            serde_json::from_str::<Value>(
                &sql(&s.fixture)
                    .query_row(
                        "SELECT body FROM drafts WHERE resource LIKE 'inline/%'",
                        [],
                        |row| row.get::<_, String>(0)
                    )
                    .unwrap()
            )
            .unwrap(),
            original
        );
        unchanged_git_and_exact_script(&s);
    }
}
