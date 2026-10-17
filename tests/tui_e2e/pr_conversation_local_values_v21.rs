//! Actual binary/input/normal autosave; no fabricated Form/source/selection.
use super::*;
const TEXT: &str = "V21 corrected exact body café 界\nSecond retained line";

fn all_sql(f: &Fixture) -> Vec<String> {
    let db = raw(f);
    let names=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>();
    let mut out = Vec::new();
    for name in names {
        let mut statement = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let count = statement.column_count();
        let mut rows = statement
            .query_map([], |r| {
                let values = (0..count)
                    .map(|i| r.get::<_, rusqlite::types::Value>(i))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(format!("{name}:{values:?}"))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        rows.sort();
        out.extend(rows);
    }
    out
}
fn draft_current(s: &mut Session, f: &Fixture, key: &str, value: &str) -> (String, String) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        let rows=raw(f).prepare("SELECT resource,body FROM drafts WHERE resource LIKE 'conversation-draft-v17/%' ORDER BY resource").unwrap().query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).unwrap().map(Result::unwrap).collect::<Vec<_>>();
        assert!(
            rows.len() <= 1,
            "Fresh actual action must not create multiple draft identities"
        );
        if let Some((resource, body)) = rows.into_iter().next() {
            let decoded: Value = serde_json::from_str(&body).unwrap();
            if decoded["values"][key] == value {
                assert_eq!(decoded["version"], 17);
                assert!(decoded["publication"].is_null());
                return (resource, body);
            }
        }
        assert!(
            Instant::now() < until,
            "Normal autosave must actually finish exact current desired value before SQL witness"
        );
        s.poll_output(Duration::from_millis(2));
    }
}
fn field(command: PrCommand) -> (&'static str, &'static str, &'static str, &'static str) {
    match command {
        PrCommand::NewComment | PrCommand::EditComment => (
            "Comment text",
            "body",
            "   ",
            "Comment must contain text and fit within 65536 UTF-8 bytes.",
        ),
        PrCommand::AddReaction => (
            "Reaction",
            "reaction",
            "not-a-reaction",
            "Choose a supported reaction from the visible list.",
        ),
        PrCommand::RemoveReaction => (
            "My exact existing reaction",
            "reaction_id",
            "602",
            "Choose an exact loaded reaction by the verified account.",
        ),
        PrCommand::Subscription => (
            "Requested subscription state",
            "subscription",
            "not-a-state",
            "Choose a supported subscription state.",
        ),
        _ => unreachable!(),
    }
}
fn invalid_then_corrected(mouse: bool) {
    for personal in [false, true] {
        for command in SIX {
            let mut steps = initial_steps(personal);
            let initial = steps.len();
            steps.extend(phase(command, true, Some(true)));
            let opened = steps.len();
            steps.extend(phase(command, false, Some(true)));
            let prepared = steps.len();
            let f = resource_configured(&steps);
            for uid in [42, 43] {
                Cache::open(&database(&f))
                    .unwrap()
                    .put_draft(
                        &Scope::github(HOST, Some(uid)).unwrap(),
                        "independent-kept",
                        &format!("KEPT_{uid}"),
                    )
                    .unwrap();
            }
            let git = git_state(&f);
            let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
            let mut s = launch(&f, mouse);
            enter(
                &mut s,
                &f,
                personal,
                mouse,
                initial,
                !matches!(command, PrCommand::NewComment | PrCommand::Subscription),
                &steps[..initial],
            );
            s.resize(120, 40);
            let before = s.completed_cells_end();
            crate::pr_actions::menu(
                &mut s,
                mouse,
                if command == PrCommand::NewComment {
                    "New comment • eligibility UNKNOWN/server-enforced"
                } else {
                    command.label()
                },
            );
            s.wait_for_command_count(&f, opened);
            form_frame(&mut s, before, &title(command), controls(command));
            let independent = stable_sql(&f);
            if command != PrCommand::DeleteComment {
                let (label, key, invalid, guidance) = field(command);
                if mouse {
                    if matches!(command, PrCommand::NewComment | PrCommand::EditComment) {
                        s.click_owned_form_field("PR INTENT • Preview first", label);
                    } else {
                        // Choice fields render `label • value`, not the colon
                        // text-editor/caret geometry. Use their actual complete
                        // button inside this exact completed owned Form instead.
                        let current = match command {
                            PrCommand::AddReaction => "[+1]",
                            PrCommand::RemoveReaction => "[heart • @octocat • reaction 601]",
                            PrCommand::Subscription => "[unsubscribed]",
                            _ => unreachable!(),
                        };
                        let (column, row) = s.wait_for_owned_completed_action(
                            &title(command),
                            current,
                            &["READ-ONLY PREVIEW"],
                        );
                        s.click(column, row);
                    }
                }
                // Choice editors really accept Ctrl-U/typed text; these are actual
                // native inputs, not a poisoned DTO or a forged choice action.
                s.send_key(b"\x15");
                if matches!(command, PrCommand::NewComment | PrCommand::EditComment) {
                    s.paste(invalid);
                } else {
                    s.send_key(invalid.as_bytes());
                }
                let (draft_key, encoded) = draft_current(&mut s, &f, key, invalid);
                let sql = all_sql(&f);
                let before = s.completed_cells_end();
                control(
                    &mut s,
                    mouse,
                    &title(command),
                    "[Preview]",
                    controls(command),
                );
                form_frame(&mut s, before, &title(command), &[guidance, "[Preview]"]);
                assert_eq!(
                    all_sql(&f),
                    sql,
                    "Invalid request must not queue SQL/ACK/journal or change current reservations"
                );
                exact(&s, &f, &steps[..opened]);
                let actual: String = raw(&f)
                    .query_row(
                        "SELECT body FROM drafts WHERE resource=?1",
                        [&draft_key],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert_eq!(actual, encoded);
                assert!(!s.screen().contains("Original Conversation retired"));
                assert!(!s.screen().contains("READ-ONLY PREVIEW"));
                if matches!(command, PrCommand::NewComment | PrCommand::EditComment) {
                    if mouse {
                        s.click_owned_form_field("PR INTENT • Preview first", label);
                    } else {
                        s.send_key(b"\x1b[Z");
                    }
                    s.send_key(b"\x15");
                    s.paste(TEXT);
                    let (same_key, _) = draft_current(&mut s, &f, key, TEXT);
                    assert_eq!(same_key, draft_key);
                    let sql = all_sql(&f);
                    let before = s.completed_cells_end();
                    s.paste(&"x".repeat(65_537));
                    form_frame(
                        &mut s,
                        before,
                        &title(command),
                        &[
                            "Conversation body exceeds64KiB; input not adopted",
                            TEXT.lines().next().unwrap(),
                        ],
                    );
                    assert_eq!(all_sql(&f), sql);
                    exact(&s, &f, &steps[..opened]);
                    assert_eq!(draft_current(&mut s, &f, key, TEXT).0, draft_key);
                } else {
                    let (caption, value) = match command {
                        PrCommand::AddReaction => ("[heart]", "heart"),
                        PrCommand::RemoveReaction => ("[heart • @octocat • reaction 601]", "601"),
                        PrCommand::Subscription => ("[ignored]", "ignored"),
                        _ => unreachable!(),
                    };
                    if mouse {
                        let (column, row) = s.wait_for_owned_completed_action(
                            &title(command),
                            caption,
                            &["READ-ONLY PREVIEW"],
                        );
                        s.click(column, row);
                    } else {
                        // The keyboard helper focused Preview, so one real BackTab
                        // returns to the actual editable field before cycling it.
                        s.send_key(b"\x1b[Z");
                        for _ in 0..if command == PrCommand::AddReaction {
                            5
                        } else if command == PrCommand::Subscription {
                            3
                        } else {
                            1
                        } {
                            s.send_key(b" ");
                        }
                    }
                    assert_eq!(draft_current(&mut s, &f, key, value).0, draft_key);
                }
            }
            let before = s.completed_cells_end();
            let mut footer = controls(command).to_vec();
            if command != PrCommand::DeleteComment {
                // The current error remains visible while values are edited;
                // real Tab traversal includes its actual painted controls.
                footer.extend(["[F7 Error↑]", "[F8 Error↓]"]);
            }
            control(&mut s, mouse, &title(command), "[Preview]", &footer);
            s.wait_for_command_count(&f, prepared);
            form_frame(
                &mut s,
                before,
                &title(command),
                &["READ-ONLY PREVIEW", "[Confirm action]"],
            );
            s.capture_completed_terminal_v1("conversation-local-values-corrected-preview-v21");
            s.paste("POISON_AT_VALIDATED_PREVIEW_V21");
            assert!(!s.screen().contains("POISON_AT_VALIDATED_PREVIEW_V21"));
            assert_eq!(
                raw(&f)
                    .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                        .get::<_, usize>(0))
                    .unwrap(),
                0
            );
            assert_eq!(
                raw(&f)
                    .query_row(
                        "SELECT count(*) FROM pr_write_conversation_origins_v17",
                        [],
                        |r| r.get::<_, usize>(0)
                    )
                    .unwrap(),
                0
            );
            assert_eq!(stable_sql(&f), independent);
            exact(&s, &f, &steps);
            assert_eq!(git_state(&f), git);
            assert_read_only_git(&f);
            assert_eq!(
                fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
                config
            );
            let before = s.completed_cursorless_end();
            control(
                &mut s,
                mouse,
                &title(command),
                "[Keep draft / Close]",
                controls(command),
            );
            s.wait_for_new_cursorless_condition(
                before,
                |screen| {
                    crate::support::main_footer_visible(screen)
                        && !screen.contents().contains("[Confirm action]")
                },
                "Actual pure Close after validated read-only Preview",
                Duration::from_secs(8),
            );
            s.quit_and_assert_restored_terminal(mouse);
            exact(&s, &f, &steps);
        }
    }
}
#[test]
fn conversation_comments_v21_keyboard_actual_six_local_invalid_correction_and_bodyless_preview() {
    invalid_then_corrected(false);
}
#[test]
fn conversation_comments_v21_mouse_actual_six_local_invalid_correction_and_bodyless_preview() {
    invalid_then_corrected(true);
}
