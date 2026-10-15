//! DECLARED/UNRUN. Genuine server Review/actors -> bounded shared Form Preview;
//! also held original Preview cancellation. No HTTP writes in this cohort.
use super::*;
fn summary() -> String {
    (0..150)
        .map(|row| format!("REVIEW_ROW_{row:03} {} 界e\u{301} ", "abcdefgh".repeat(12)))
        .collect::<String>()
        + "EXACT_REVIEW_END"
}
fn marker(session: &Session) -> Option<String> {
    let text = session.completed_cursorless_screen()?.contents();
    let start = text.find("REVIEW_ROW_")?;
    Some(text[start..].chars().take(14).collect())
}
fn scroll(session: &mut Session, mouse: bool) {
    let old = session.completed_cursorless_end();
    let previous = session.completed_cursorless_screen().unwrap().contents();
    if mouse {
        session.wheel(30, 12, true)
    } else {
        session.send_key(b"\x1b[6~")
    }
    session.wait_for_new_cursorless_condition(
        old,
        |frame| {
            let text = frame.contents();
            text != previous
                && text.contains("READ-ONLY PREVIEW")
                && text.contains("[Confirm action]")
        },
        "actual visible-only Review scroll",
        Duration::from_secs(8),
    );
}
fn journey(mouse: bool) {
    for mode in [0, 1, 2] {
        let cancel_pending = mode == 1;
        let changed_uid = mode == 2;
        let (fixture, h, mut steps) = fixture_and_steps(|_| {});
        steps.truncate(steps.len() - 17);
        steps.extend(source_open(
            &h,
            json!([row(&h, Effect::Dismiss, Failure::None, false)]),
        ));
        steps.extend(selection(&h, Effect::Dismiss, Failure::None, "RIGHT"));
        steps.extend(source_open(
            &h,
            json!([row(&h, Effect::Dismiss, Failure::None, false)]),
        ));
        let mut preview = opening(&h);
        preview.extend(snapshot(&h, Effect::Dismiss, Failure::None, false, "RIGHT"));
        preview.extend(closing(&h));
        steps.extend(preview.clone());
        let prepared_calls = steps.len();
        if cancel_pending {
            let mut held = preview.remove(0);
            held.delay_ms = 6000;
            steps.push(held);
        }
        if changed_uid {
            *preview.last_mut().unwrap() = get("user", json!({"id":43,"login":"octocat"}));
            steps.extend(preview);
        }
        fixture.scripted_gh(&steps);
        foreign_draft(&fixture);
        let git = git_state(&fixture);
        let drafts = stable_drafts(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = launch(&fixture, mouse);
        files_selected(&mut session, &fixture, mouse);
        let trace = git_commands(&fixture);
        let old = session.completed_cursorless_end();
        choose(&mut session, mouse, "[F10 Reviews]", b"\x1b[21~");
        browser_ready(&mut session, old, "#810");
        let old = session.completed_cursorless_end();
        choose(&mut session, mouse, "[R Resume selected]", b"R");
        browser_ready(&mut session, old, "Actual existing server pending comment");
        choose(
            &mut session,
            mouse,
            "[I Dismiss: eligibility UNKNOWN]",
            b"I",
        );
        edited_field(
            &mut session,
            "Dismissal reason (server privilege UNKNOWN):",
            "",
        );
        if mouse {
            session.click_owned_form_field(
                "PR INTENT • Preview first",
                "Dismissal reason (server privilege UNKNOWN)",
            );
        }
        let original = summary();
        session.paste(&original);
        session
            .try_wait_for_condition(
                |text| {
                    text.contains("PR INTENT • Preview first")
                        && text.contains("EXACT_REVIEW_END")
                        && text.contains("[Preview]")
                },
                "actual long Review reason/caret before Preview",
                Duration::from_secs(8),
            )
            .unwrap();
        let old = session.completed_cursorless_end();
        choose(&mut session, mouse, "[Preview]", b"\x10");
        session.wait_for_new_cursorless_condition(
            old,
            |frame| {
                let t = frame.contents();
                t.contains("READ-ONLY PREVIEW")
                    && t.contains("[Confirm action]")
                    && t.contains("Current H")
                    && t.contains(&h.h)
            },
            "actual original Review Preview H and exact complete controls",
            Duration::from_secs(8),
        );
        assert_eq!(commands(&session, &fixture).len(), prepared_calls);
        for _ in 0..18 {
            scroll(&mut session, mouse);
        }
        let anchor = marker(&session).expect("actual deep summary marker");
        for (width, height) in [(80, 24), (120, 40), (80, 24), (120, 40)] {
            let old = session.completed_cursorless_end();
            session.resize(width, height);
            session.wait_for_new_cursorless_condition(
                old,
                |frame| {
                    frame.contents().contains(&anchor)
                        && frame.contents().contains("[Confirm action]")
                },
                "exact Review byte-anchor through real width reflow",
                Duration::from_secs(8),
            );
        }
        // Seek actual retained comment-author header, not an invented snapshot
        // or a requester-derived actor. Each step has a NEW completed witness.
        for _ in 0..240 {
            if session
                .completed_cursorless_screen()
                .unwrap()
                .contents()
                .contains("Observed comment #911")
            {
                break;
            }
            scroll(&mut session, mouse);
        }
        ready(
            &mut session,
            &["Observed comment #911", "actual-review-author"],
        );
        assert_eq!(commands(&session, &fixture).len(), prepared_calls);
        assert_eq!(
            raw(&fixture)
                .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| row
                    .get::<_, u64>(0))
                .unwrap(),
            0
        );
        let old = session.completed_cursorless_end();
        // This modal has ONE complete control on its bottom row, not all
        // three controls on that row. Prove that resize anchor, then prove
        // each other complete control inside the same owned completed box.
        session.resize_owned_single_action_modal_v2(
            40,
            24,
            "Preview • resize required",
            "[Esc Close]",
        );
        for label in ["[Ctrl+P Edit fields]", "[Ctrl+Enter Confirm unavailable]"] {
            session.wait_for_owned_completed_action("Preview • resize required", label, &[]);
        }
        session.wait_for_new_cursorless_condition(
            old,
            |frame| {
                let t = frame.contents();
                t.contains("resize required")
                    && t.contains("[Ctrl+P Edit fields]")
                    && t.contains("[Ctrl+Enter Confirm unavailable]")
                    && t.contains("[Esc Close]")
            },
            "Review resize-only preserves accessible fields and disabled confirmation",
            Duration::from_secs(8),
        );
        if mouse {
            session.click_visible_action("[Ctrl+Enter Confirm unavailable]")
        } else {
            session.send_key(b"\x1b[13;5u")
        }
        session.wait_for_cursorless_condition(
            |frame| frame.contents().contains("Confirm refused"),
            "actual Review hidden Confirm rejected",
            Duration::from_secs(8),
        );
        if mouse {
            session.click_visible_action("[Ctrl+P Edit fields]")
        } else {
            session.send_key(b"\x10")
        }
        session.resize(120, 40);
        session
            .try_wait_for_condition(
                |text| {
                    text.contains("PR INTENT • Preview first")
                        && text.contains("EXACT_REVIEW_END")
                        && text.contains("[Preview]")
                },
                "pure Back keeps exact long Review field",
                Duration::from_secs(8),
            )
            .unwrap();
        if cancel_pending || changed_uid {
            // A genuine edit invalidates old display/Preview before fresh work.
            if mouse {
                session.click_owned_form_field(
                    "PR INTENT • Preview first",
                    "Dismissal reason (server privilege UNKNOWN)",
                );
            }
            session.send_key(b"\x01");
            session.paste("Actual newer Review value before canceled Preview");
            edited_field(
                &mut session,
                "Dismissal reason (server privilege UNKNOWN):",
                "Actual newer Review value",
            );
            choose(&mut session, mouse, "[Preview]", b"\x10");
            session.wait_for_command_count(&fixture, steps.len());
            if cancel_pending {
                let pid = commands(&session, &fixture)[prepared_calls]["pid"]
                    .as_i64()
                    .unwrap() as i32;
                let identity = process_identity(pid)
                    .unwrap()
                    .expect("actual delayed original Preview child");
                let owner =
                    OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
                assert!(owner.owns(&identity) && !identity.zombie);
                choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
                session.wait_for_cursorless_condition(
                    |frame| {
                        let t = frame.contents();
                        t.contains("operation canceled")
                            && !t.contains("REVIEW_ROW_")
                            && !t.contains("Actual newer Review value")
                    },
                    "owned canceled original Preview retires private underlay",
                    Duration::from_secs(8),
                );
                let deadline = Instant::now() + Duration::from_secs(5);
                while let Some(now) = process_identity(pid).unwrap() {
                    assert_eq!(
                        (now.uid, now.birth, now.session),
                        (identity.uid, identity.birth, identity.session)
                    );
                    assert!(Instant::now() < deadline);
                    session.poll_output(Duration::from_millis(10));
                }
            } else {
                session.wait_for_cursorless_condition(|frame|{let t=frame.contents();t.contains("Original review source unavailable")&&!t.contains("REVIEW_ROW_")&&!t.contains("Actual newer Review value")&&!t.contains("old-no-newline")},"same login but changed actual numericUID hides all original private Preview/underlay",Duration::from_secs(8));
                assert_eq!(
                    raw(&fixture)
                        .query_row(
                            "SELECT count(*) FROM records WHERE login='user-id:43'",
                            [],
                            |row| row.get::<_, u64>(0)
                        )
                        .unwrap(),
                    0
                );
            }
            let saved:String=raw(&fixture).query_row("SELECT body FROM drafts WHERE login='user-id:42' AND resource LIKE 'review-draft/%'",[],|row|row.get(0)).unwrap();
            let saved: Value = serde_json::from_str(&saved).unwrap();
            assert_eq!(
                saved["values"]["summary"],
                "Actual newer Review value before canceled Preview"
            );
            assert!(saved.get("approved").is_none());
        } else {
            // Close changes the real global footer (Reviews becomes available).
            // Prove its NEW completed frame before locating one genuine Quit.
            kept(&mut session, mouse);
        }
        exact_commands(&session, &fixture, &steps, 0);
        assert_eq!(stable_drafts(&fixture), drafts);
        assert_eq!(git_state(&fixture), git);
        assert_eq!(git_commands(&fixture), trace);
        assert_eq!(
            fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
            config
        );
        session.quit_and_assert_restored_terminal(mouse);
    }
}
#[test]
fn form_preview_visible_keyboard_review_author_rows_resize_disabled_back_and_owned_pending_cancel()
{
    journey(false)
}
#[test]
fn form_preview_visible_mouse_review_author_rows_resize_disabled_back_and_owned_pending_cancel() {
    journey(true)
}
