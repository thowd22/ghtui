//! SOURCE ONLY / UNRUN: twelve real-binary tests, each actual Catalogue AND
//! Personal queue, denied invocation -> restart -> fresh Inspect/local ACK ->
//! restart -> SAME deliberate action/New -> fresh current Preview+Confirm.
//! No handler injection, SQL-created ACK, source promotion or extra publisher.
use super::*;
const NOTICE: &[&str] = &["[n New action]", "[Inspect history]", "[Esc Close]"];

pub(super) fn inspect_steps(command: PrCommand) -> Vec<GhStep> {
    let mut calls = phase(PrCommand::NewComment, false, Some(true));
    let mut actual = match command {
        PrCommand::NewComment => vec![get(
            &format!("repos/{FIRST}/issues/7/comments?per_page=100&page=1"),
            json!([comment(501, OLD)]),
        )],
        PrCommand::Subscription => vec![graphql(
            ghtui::providers::gh::PR_CURRENT_SUBSCRIPTION_QUERY,
            json!({"id":"GENUINE_CANONICAL_PR_NODE_1007"}),
            subscription("UNSUBSCRIBED"),
        )],
        _ => vec![
            graphql(
                COMMENT_QUERY,
                json!({"id":"IC_501"}),
                capabilities(Some(true)),
            ),
            get(
                &format!("repos/{FIRST}/issues/comments/501"),
                comment(501, OLD),
            ),
        ],
    };
    if matches!(command, PrCommand::AddReaction | PrCommand::RemoveReaction) {
        actual.push(get(
            &format!("repos/{FIRST}/issues/comments/501/reactions?per_page=100&page=1"),
            reactions(),
        ));
    }
    calls.splice(4..4, actual);
    calls
}
fn current_page(
    s: &mut Session,
    f: &Fixture,
    mouse: bool,
    personal: bool,
    start: usize,
    command: PrCommand,
    expected: &[GhStep],
) {
    let end = start + initial_steps(personal).len();
    // Delegate the SAME actual route and complete cumulative argv/stdin
    // observer used by the current V109 journeys. Earlier real POST input must
    // not be passed through the shared initial read-only catalogue helper.
    super::enter(
        s,
        f,
        personal,
        mouse,
        end,
        !matches!(command, PrCommand::NewComment | PrCommand::Subscription),
        expected,
    );
    assert_eq!(commands(s, f).len(), end, "Pure tab/selection is read only");
}
fn safe(s: &mut Session, needle: &str) {
    if needle == "Confirmed; refresh explicitly" {
        refreshed_v19::wait_fresh_page_v19(s);
        return;
    }
    if needle == "Confirmed; visibility unavailable; no resend" {
        form_frame(
            s,
            0,
            "Confirmed Conversation • read-only refresh",
            &["DURABLY CONFIRMED", "[r Refresh PR]", "[Esc Back to Inbox]"],
        );
        return;
    }
    s.wait_for_cursorless_condition(
        |screen| {
            crate::support::main_footer_visible(screen)
                && screen.contents().contains(needle)
                && !screen.contents().contains(BODY)
                && !screen.contents().contains(OLD)
                && !screen.contents().contains(DETAIL_BODY)
                && !screen.contents().contains("[Confirm action]")
        },
        "Actual terminal main outcome; not a busy Form or matching Preview phrase",
        Duration::from_secs(8),
    );
}
fn expected_queue_sql(
    f: &Fixture,
    personal: bool,
    original: &[String],
    generation: u64,
) -> Vec<String> {
    assert!((1..=5).contains(&generation));
    let repositories = if personal {
        Vec::<&str>::new()
    } else {
        vec![FIRST]
    };
    let source = format!(
        "personal-inbox:{}",
        json!({"excluded":[],"repositories":repositories})
    );
    let checkpoint:(u64,i64) = raw(f).query_row(
        "SELECT generation,complete FROM checkpoints WHERE host=?1 AND login='user-id:42' AND source=?2",
        [HOST,source.as_str()], |r|Ok((r.get(0)?,r.get(1)?)),
    ).unwrap();
    assert_eq!(
        checkpoint,
        (generation, 1),
        "Only the known explicit queue refresh can advance this checkpoint"
    );
    let initial = json!([HOST, "user-id:42", source, 1, 1]).to_string();
    let current = json!([HOST, "user-id:42", source, generation, 1]).to_string();
    assert_eq!(original.iter().filter(|row| *row == &initial).count(), 1);
    let expected = original
        .iter()
        .map(|row| {
            if row == &initial {
                current.clone()
            } else {
                row.clone()
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        stable_sql(f),
        expected,
        "Entire stable SQL, not an observed post-operation baseline"
    );
    expected
}
pub(super) fn scroll(s: &mut Session, mouse: bool, title: &str, needle: &str) {
    let until = Instant::now() + Duration::from_secs(8);
    let compact = |text: &str| {
        text.chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
    };
    let expected = compact(needle);
    let visible = |s: &Session| {
        s.completed_cells_screen().is_some_and(|screen| {
            crate::support::overlay_body(screen, title)
                .is_some_and(|body| compact(&body).contains(&expected))
        })
    };
    let mut scroll_frame = None;
    loop {
        s.poll_output(Duration::from_millis(5));
        if scroll_frame.is_some_and(|frame| s.completed_cells_end() <= frame) {
            continue;
        }
        if visible(s) {
            return;
        }
        assert!(
            Instant::now() < until,
            "Exact current original observation must be inspectable: {needle}\n{}",
            s.screen()
        );
        if mouse {
            let p = s.wait_for_owned_completed_action(title, "[Esc Close]", &[]);
            if visible(s) {
                return;
            }
            scroll_frame = Some(s.completed_cells_end());
            s.wheel(p.0, p.1.saturating_sub(3), true);
        } else {
            scroll_frame = Some(s.completed_cells_end());
            s.send_key(b"\x1b[6;5~");
        }
    }
}
fn populate(s: &mut Session, mouse: bool, command: PrCommand) {
    match command {
        PrCommand::NewComment | PrCommand::EditComment => {
            if mouse {
                s.click_owned_form_field("PR INTENT • Preview first", "Comment text");
            }
            s.send_key(b"\x01");
            s.paste(BODY);
            s.wait_for_screen(BODY);
        }
        PrCommand::AddReaction => {
            if mouse {
                s.click_visible_action("[heart]");
            } else {
                for _ in 0..5 {
                    s.send_key(b" ");
                }
            }
        }
        PrCommand::Subscription => {
            if mouse {
                s.click_visible_action("[ignored]");
            } else {
                s.send_key(b" ");
            }
        }
        PrCommand::RemoveReaction => {
            assert!(!s.screen().contains("reaction 602"));
            if mouse {
                s.click_visible_action("[heart • @octocat • reaction 601]");
            } else {
                s.send_key(b" ");
            }
        }
        _ => {}
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum ModeV18 {
    Publish,
    CancelNew,
    AccountLoss,
}
fn run(command: PrCommand, mouse: bool, personal: bool, mode: ModeV18) {
    let mut steps = initial_steps(personal);
    let first = steps.len();
    steps.extend(phase(command, true, Some(true)));
    let opened = steps.len();
    steps.extend(phase(command, false, Some(true)));
    let prepared = steps.len();
    steps.extend(phase(command, false, Some(true)));
    let mut denied = receipt(command).remove(0);
    denied.status = 1;
    denied.stdout.clear();
    denied.stderr = b"HTTP403 explicit V18 first invocation denied".to_vec();
    steps.push(denied);
    let denied_end = steps.len();
    steps.extend(initial_steps(personal));
    let second = steps.len();
    steps.extend(phase(PrCommand::NewComment, false, Some(true)));
    let history = steps.len();
    steps.extend(inspect_steps(command));
    let inspected = steps.len();
    steps.extend(initial_steps(personal));
    let third = steps.len();
    steps.extend(phase(command, true, Some(true)));
    let noticed = steps.len();
    let mut later = phase(command, true, Some(true));
    if mode == ModeV18::CancelNew {
        later.last_mut().unwrap().delay_ms = 30_000;
    }
    if mode == ModeV18::AccountLoss {
        *later.last_mut().unwrap() = get("user", json!({"id":43,"login":"octocat"}));
    }
    steps.extend(later);
    let fresh = steps.len();
    if mode == ModeV18::Publish {
        steps.extend(phase(command, false, Some(true)));
    }
    let previewed = steps.len();
    if mode == ModeV18::Publish {
        steps.extend(phase(command, false, Some(true)));
        steps.extend(receipt(command));
        steps.extend(visibility_for_v19(command));
    }
    let f = resource_configured(&steps);
    let git = git_state(&f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    for uid in [42, 43] {
        Cache::open(&database(&f))
            .unwrap()
            .put_draft(
                &Scope::github(HOST, Some(uid)).unwrap(),
                "v18-independent-original",
                &format!("PRIVATE_UNRELATED_{uid}"),
            )
            .unwrap();
    }
    let mut s = launch(&f, mouse);
    current_page(&mut s, &f, mouse, personal, 0, command, &steps[..first]);
    assert_eq!(commands(&s, &f).len(), first);
    let original_stable = stable_sql(&f);
    let mut stable = expected_queue_sql(&f, personal, &original_stable, 1);
    prepare(&mut s, &f, mouse, command, opened, prepared);
    control(
        &mut s,
        mouse,
        &title(command),
        "[Confirm action]",
        controls(command),
    );
    s.wait_for_command_count(&f, denied_end);
    safe(&mut s, "Unknown outcome; inspect");
    let (old_id,old_key,old_body,old_origin):(i64,String,String,String)=raw(&f).query_row("SELECT a.attempt_id,d.resource,d.body,o.origin_json FROM pr_write_attempts a JOIN pr_write_conversation_origins_v17 o USING(attempt_id) JOIN drafts d ON d.resource LIKE 'conversation-draft-v17/%' WHERE a.state='uncertain'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
    assert!(serde_json::from_str::<Value>(&old_body).unwrap()["publication"].is_object());
    exact(&s, &f, &steps[..denied_end]);
    assert_eq!(stable_sql(&f), stable);
    s.quit_and_assert_restored_terminal(mouse);

    let mut s = launch(&f, mouse);
    current_page(
        &mut s,
        &f,
        mouse,
        personal,
        denied_end,
        command,
        &steps[..second],
    );
    assert_eq!(commands(&s, &f).len(), second);
    stable = expected_queue_sql(&f, personal, &original_stable, 2);
    let history_title = format!("Original Conversation history • {FIRST}#7");
    let frame = s.completed_cells_end();
    crate::pr_actions::menu(&mut s, mouse, "Inspect pending result");
    s.wait_for_command_count(&f, history);
    form_frame(&mut s, frame, &history_title, HISTORY);
    let frame = s.completed_cells_end();
    control(&mut s, mouse, &history_title, "[Inspect current]", HISTORY);
    s.wait_for_command_count(&f, inspected);
    form_frame(&mut s, frame, &history_title, HISTORY);
    scroll(
        &mut s,
        mouse,
        &history_title,
        match command {
            PrCommand::NewComment => "Original created comment identity was never observed; UNKNOWN attribution. No first/newest comment is chosen.",
            PrCommand::Subscription => "not notification delivery",
            PrCommand::AddReaction | PrCommand::RemoveReaction => {
                "Complete current reaction catalogue"
            }
            _ => "Current comment #501",
        },
    );
    control(
        &mut s,
        mouse,
        &history_title,
        "[Acknowledge local]",
        HISTORY,
    );
    safe(&mut s, "Local ACK #");
    assert_eq!(
        raw(&f)
            .query_row(
                "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
                [old_id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "acknowledged"
    );
    assert_eq!(
        raw(&f)
            .query_row(
                "SELECT body FROM drafts WHERE resource=?1",
                [&old_key],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        old_body
    );
    exact(&s, &f, &steps[..inspected]);
    assert_eq!(stable_sql(&f), stable);
    s.quit_and_assert_restored_terminal(mouse);

    let mut s = launch(&f, mouse);
    current_page(
        &mut s,
        &f,
        mouse,
        personal,
        inspected,
        command,
        &steps[..third],
    );
    assert_eq!(commands(&s, &f).len(), third);
    stable = expected_queue_sql(&f, personal, &original_stable, 3);
    let frame = s.completed_cells_end();
    crate::pr_actions::menu(
        &mut s,
        mouse,
        if command == PrCommand::NewComment {
            "New comment • eligibility UNKNOWN/server-enforced"
        } else {
            command.label()
        },
    );
    s.wait_for_command_count(&f, noticed);
    form_frame(&mut s, frame, &title(command), NOTICE);
    assert_eq!(commands(&s, &f).len(), noticed);
    assert_eq!(
        raw(&f)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, usize>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        raw(&f)
            .query_row(
                "SELECT body FROM drafts WHERE resource=?1",
                [&old_key],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        old_body
    );
    // No hidden New target in tiny Preview. Both the actual disabled
    // footer click and n are refused; no queue/write/ref/body changes.
    let before = s.completed_cells_end();
    s.resize(40, 10);
    form_frame(
        &mut s,
        before,
        "Preview • resize required",
        &["[Ctrl+Enter Confirm unavailable]", "[Esc Close]"],
    );
    if mouse {
        let p = s.wait_for_owned_completed_action(
            "Preview • resize required",
            "[Ctrl+Enter Confirm unavailable]",
            &[],
        );
        s.click(p.0, p.1);
    } else {
        s.send_key(b"n");
    }
    assert_eq!(commands(&s, &f).len(), noticed);
    let before = s.completed_cells_end();
    s.resize(120, 40);
    form_frame(&mut s, before, &title(command), NOTICE);
    let before = s.completed_cells_end();
    control(&mut s, mouse, &title(command), "[n New action]", NOTICE);
    s.wait_for_command_count(&f, fresh);
    if mode != ModeV18::Publish {
        if mode == ModeV18::CancelNew {
            form_frame(&mut s, before, &title(command), &["[Esc Cancel]"]);
            let rows = commands(&s, &f);
            let pid = i32::try_from(rows[fresh - 1]["pid"].as_u64().unwrap()).unwrap();
            let process = process_identity(pid).unwrap().unwrap();
            let owned = OwnedPtySession::capture(s.owned_application_pid() as u32).unwrap();
            assert!(owned.owns(&process));
            assert!(!process.zombie);
            control(
                &mut s,
                mouse,
                &title(command),
                "[Esc Cancel]",
                &["[Esc Cancel]"],
            );
            s.wait_for_cursorless_condition(
                |screen| {
                    crate::support::main_footer_visible(screen)
                        && screen.contents().contains("Conversation canceled")
                        && screen.contents().contains(DETAIL_BODY)
                        && !screen.contents().contains(BODY)
                },
                "Actual New-action cancellation restores only the still-current original page",
                Duration::from_secs(8),
            );
            let until = Instant::now() + Duration::from_secs(5);
            loop {
                match process_identity(pid) {
                    Ok(None) => break,
                    Ok(Some(current)) => assert_eq!(
                        (current.pid, current.uid, current.birth, current.session),
                        (process.pid, process.uid, process.birth, process.session)
                    ),
                    #[cfg(target_os = "linux")]
                    Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {}
                    Err(error) => panic!("Exact New-read process observation failed: {error}"),
                }
                assert!(
                    Instant::now() < until,
                    "Exact original New-read child must be reaped"
                );
                s.poll_output(Duration::from_millis(5));
            }
            // Clear is the actual cached-INBOX control. The canceled New
            // phase correctly leaves its original PR page open; keyboard x
            // also works there, but there is no corresponding mouse button.
            // Exercise the SAME visible action semantics for both inputs.
            let frame = s.completed_cursorless_end();
            choose(&mut s, mouse, "[b Back]", b"b");
            s.wait_for_new_cursorless_condition(
                frame,
                |screen| {
                    let text = screen.contents();
                    crate::support::main_footer_visible(screen)
                        && text.contains("Inbox • focused view")
                        && text.contains("[x Clear cache]")
                        && !text.contains("[p PR actions]")
                        && !text.contains(DETAIL_BODY)
                        && !text.contains(BODY)
                },
                "Pure cached Back completes before the actual visible Inbox Clear action",
                Duration::from_secs(8),
            );
            assert_eq!(
                commands(&s, &f).len(),
                fresh,
                "Cached Back must not start GH work"
            );
            assert_eq!(
                stable_sql(&f),
                stable,
                "Cached Back changes no SQL projection/draft"
            );
            let frame = s.completed_cursorless_end();
            choose(&mut s, mouse, "[x Clear cache]", b"x");
            let cleared_view = if personal {
                "No pull requests"
            } else {
                "Original account cache cleared • drafts and write history retained"
            };
            s.wait_for_new_cursorless_condition(
                frame,
                |screen| {
                    crate::support::main_footer_visible(screen)
                        && screen.contents().contains(cleared_view)
                        && !screen.contents().contains(DETAIL_BODY)
                        && !screen.contents().contains(OLD)
                        && !screen.contents().contains(BODY)
                        && !screen.contents().contains("[p PR actions]")
                },
                "Exact origin-specific completed Clear view; independent SQL must prove deletion, protected drafts/origins survive",
                Duration::from_secs(8),
            );
            for table in ["records", "checkpoints"] {
                assert_eq!(
                    raw(&f)
                        .query_row(
                            &format!(
                                "SELECT count(*) FROM {table} WHERE host=?1 AND login='user-id:42'"
                            ),
                            [HOST],
                            |r| r.get::<_, usize>(0)
                        )
                        .unwrap(),
                    0
                );
            }
        } else {
            safe(&mut s, "Original Conversation unavailable");
        }
        let db = raw(&f);
        assert_eq!(
            db.query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, usize>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            db.query_row(
                "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
                [old_id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "acknowledged"
        );
        assert_eq!(
            db.query_row(
                "SELECT body FROM drafts WHERE resource=?1",
                [&old_key],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            old_body
        );
        assert_eq!(
            db.query_row(
                "SELECT origin_json FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1",
                [old_id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            old_origin
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM drafts WHERE resource LIKE '%/new-v18/%'",
                [],
                |r| r.get::<_, usize>(0)
            )
            .unwrap(),
            0
        );
        for uid in [42, 43] {
            assert_eq!(
                Cache::open(&database(&f))
                    .unwrap()
                    .draft(
                        &Scope::github(HOST, Some(uid)).unwrap(),
                        "v18-independent-original"
                    )
                    .unwrap()
                    .as_deref(),
                Some(format!("PRIVATE_UNRELATED_{uid}").as_str())
            );
        }
        let keep = |rows: Vec<String>| {
            rows.into_iter()
                .filter(|row| {
                    let value: Value = serde_json::from_str(row).unwrap();
                    !(value.as_array().is_some_and(|r| r.len() == 5)
                        && value[0] == HOST
                        && value[1] == "user-id:42")
                })
                .collect::<Vec<_>>()
        };
        if mode == ModeV18::CancelNew {
            assert_eq!(keep(stable_sql(&f)), keep(stable));
        } else {
            assert_eq!(stable_sql(&f), stable);
        }
        assert_eq!(git_state(&f), git);
        assert_eq!(
            fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
            config
        );
        exact(&s, &f, &steps);
        assert_read_only_git(&f);
        s.quit_and_assert_restored_terminal(mouse);
        return;
    }
    form_frame(&mut s, before, &title(command), controls(command));
    assert!(!s.screen().contains("NEEDS NEW ACTION"));
    let until = Instant::now() + Duration::from_secs(8);
    let (new_key, fresh_body) = loop {
        let value:Option<(String,String)>=raw(&f).query_row("SELECT resource,body FROM drafts WHERE resource LIKE 'conversation-draft-v17/%/new-v18/%'",[],|r|Ok((r.get(0)?,r.get(1)?))).optional().unwrap();
        if let Some(value) = value {
            break value;
        }
        assert!(
            Instant::now() < until,
            "Bodyless/fresh forms must register actual same-queue autosave state"
        );
        s.poll_output(Duration::from_millis(5));
    };
    let fresh_body: Value = serde_json::from_str(&fresh_body).unwrap();
    assert_eq!(fresh_body["publication"], Value::Null);
    if command == PrCommand::NewComment {
        assert_eq!(fresh_body["values"]["body"], "");
    }
    if command == PrCommand::EditComment {
        assert_eq!(fresh_body["values"]["body"], OLD);
        assert_ne!(fresh_body["values"]["body"], BODY);
    }
    if command == PrCommand::AddReaction {
        assert_eq!(fresh_body["values"]["reaction"], "");
    }
    if command == PrCommand::RemoveReaction {
        assert_eq!(fresh_body["values"]["reaction_id"], "");
    }
    populate(&mut s, mouse, command);
    let before = s.completed_cells_end();
    control(
        &mut s,
        mouse,
        &title(command),
        "[Preview]",
        controls(command),
    );
    s.wait_for_command_count(&f, previewed);
    form_frame(
        &mut s,
        before,
        &title(command),
        &["READ-ONLY PREVIEW", "[Confirm action]"],
    );
    s.paste("V18_POISON_MUST_NOT_INVALIDATE_PREVIEW");
    assert!(!s
        .screen()
        .contains("V18_POISON_MUST_NOT_INVALIDATE_PREVIEW"));
    assert_eq!(
        raw(&f)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, usize>(0))
            .unwrap(),
        1
    );
    control(
        &mut s,
        mouse,
        &title(command),
        "[Confirm action]",
        controls(command),
    );
    s.wait_for_command_count(&f, steps.len());
    safe(&mut s, "Confirmed; refresh explicitly");
    let db = raw(&f);
    assert_eq!(
        db.query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
            .get::<_, usize>(0))
            .unwrap(),
        2
    );
    let (new_id,state,command_name,receipt,origin,base,action,ack):(i64,String,String,String,String,String,String,i64)=db.query_row("SELECT a.attempt_id,a.state,a.command,a.confirmed_json,o.origin_json,o.base_key_v18,o.action_tag_v18,o.parent_ack_attempt_v18 FROM pr_write_attempts a JOIN pr_write_conversation_origins_v17 o USING(attempt_id) WHERE a.attempt_id<>?1",[old_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?))).unwrap();
    assert!(new_id > old_id);
    assert_eq!(state, "confirmed");
    assert_eq!(command_name, command_key(command));
    assert_eq!(action, command_key(command));
    assert_eq!(ack, old_id);
    assert_eq!(base, old_key);
    assert!(new_key.starts_with(&format!("{base}/new-v18/")));
    let recorded: Value = serde_json::from_str(&origin).unwrap();
    assert_eq!(recorded["seed"]["version"], 18);
    assert_eq!(
        recorded["seed"]["terminal"]["reference"]["attempt_id"],
        old_id
    );
    assert_eq!(recorded["seed"]["terminal"]["kind"], "Acknowledged");
    let receipt: Value = serde_json::from_str(&receipt).unwrap();
    assert_eq!(
        receipt["resource_id"],
        if command == PrCommand::Subscription {
            "ignored"
        } else if matches!(command, PrCommand::NewComment | PrCommand::AddReaction) {
            "901"
        } else if command == PrCommand::RemoveReaction {
            "601"
        } else {
            "501"
        }
    );
    assert_eq!(
        db.query_row(
            "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
            [old_id],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "acknowledged"
    );
    assert_eq!(
        db.query_row(
            "SELECT body FROM drafts WHERE resource=?1",
            [&old_key],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        old_body
    );
    assert_eq!(
        db.query_row(
            "SELECT origin_json FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1",
            [old_id],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        old_origin
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM drafts WHERE resource=?1",
            [&new_key],
            |r| r.get::<_, usize>(0)
        )
        .unwrap(),
        0
    );
    refreshed_v19::assert_published_transition_v19(&f, &stable, command, 2);
    assert_eq!(git_state(&f), git);
    assert_eq!(
        fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    exact(&s, &f, &steps);
    assert_read_only_git(&f);
    s.quit_and_assert_restored_terminal(mouse);
}
fn retained_confirmed(mouse: bool, personal: bool) {
    let command = PrCommand::NewComment;
    let mut steps = initial_steps(personal);
    let initial_end = steps.len();
    steps.extend(phase(command, true, Some(true)));
    let opened = steps.len();
    steps.extend(phase(command, false, Some(true)));
    let prepared = steps.len();
    steps.extend(phase(command, false, Some(true)));
    steps.extend(receipt(command));
    // Actual cleanup failure occurs after the SAME paid final numeric UID.
    steps.extend(visibility());
    let first_end = steps.len();
    steps.extend(initial_steps(personal));
    let restarted = steps.len();
    steps.extend(phase(command, true, Some(true)));
    let noticed = steps.len();
    steps.extend(phase(command, true, Some(true)));
    let fresh = steps.len();
    steps.extend(phase(command, false, Some(true)));
    let previewed = steps.len();
    steps.extend(phase(command, false, Some(true)));
    let mut next = receipt(command);
    next[0].stdout = serde_json::to_vec(&comment(902, BODY)).unwrap();
    next[1] = get(
        &format!("repos/{FIRST}/issues/comments/902"),
        comment(902, BODY),
    );
    steps.extend(next);
    steps.extend(visibility_with_comments_v19(json!([
        comment(501, OLD),
        comment(902, BODY)
    ])));
    let f = resource_configured(&steps);
    let git = git_state(&f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let mut s = launch(&f, mouse);
    current_page(
        &mut s,
        &f,
        mouse,
        personal,
        0,
        command,
        &steps[..initial_end],
    );
    assert_eq!(commands(&s, &f).len(), initial_end);
    let original_stable = stable_sql(&f);
    let stable = expected_queue_sql(&f, personal, &original_stable, 1);
    prepare(&mut s, &f, mouse, command, opened, prepared);
    let fault = rusqlite::Connection::open_with_flags(
        database(&f),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    fault.execute_batch("CREATE TRIGGER v18_native_cleanup_refused BEFORE DELETE ON drafts WHEN OLD.resource LIKE 'conversation-draft-v17/%' BEGIN SELECT RAISE(ABORT,'genuine retained-confirmed fixture');END;").unwrap();
    control(&mut s, mouse, &title(command), "[Confirm action]", EDITABLE);
    s.wait_for_command_count(&f, first_end);
    safe(&mut s, "Confirmed; visibility unavailable; no resend");
    let (id,key,body,origin,receipt):(i64,String,String,String,String)=raw(&f).query_row("SELECT a.attempt_id,d.resource,d.body,o.origin_json,a.confirmed_json FROM pr_write_attempts a JOIN pr_write_conversation_origins_v17 o USING(attempt_id) JOIN drafts d ON d.resource LIKE 'conversation-draft-v17/%' WHERE a.state='confirmed'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&receipt).unwrap()["resource_id"],
        "901"
    );
    assert_eq!(
        raw(&f)
            .query_row(
                "SELECT count(*) FROM pr_write_attempts WHERE state='acknowledged'",
                [],
                |r| r.get::<_, usize>(0)
            )
            .unwrap(),
        0
    );
    exact(&s, &f, &steps[..first_end]);
    assert_eq!(stable_sql(&f), stable);
    let before_back = s.completed_cursorless_end();
    control(
        &mut s,
        mouse,
        "Confirmed Conversation • read-only refresh",
        "[Esc Back to Inbox]",
        &["[r Refresh PR]", "[Esc Back to Inbox]"],
    );
    s.wait_for_new_cursorless_condition(
        before_back,
        |screen| {
            crate::support::main_footer_visible(screen)
                && screen.contents().contains("Cached Inbox only")
                && !screen.contents().contains(DETAIL_BODY)
        },
        "Pure cached Back strips the old PR before quit/restart",
        Duration::from_secs(8),
    );
    exact(&s, &f, &steps[..first_end]);
    assert_eq!(stable_sql(&f), stable);
    s.quit_and_assert_restored_terminal(mouse);
    fault
        .execute_batch("DROP TRIGGER v18_native_cleanup_refused")
        .unwrap();
    drop(fault);
    let mut s = launch(&f, mouse);
    current_page(
        &mut s,
        &f,
        mouse,
        personal,
        first_end,
        command,
        &steps[..restarted],
    );
    assert_eq!(commands(&s, &f).len(), restarted);
    let stable = expected_queue_sql(&f, personal, &original_stable, 2);
    let frame = s.completed_cells_end();
    crate::pr_actions::menu(
        &mut s,
        mouse,
        "New comment • eligibility UNKNOWN/server-enforced",
    );
    s.wait_for_command_count(&f, noticed);
    form_frame(&mut s, frame, &title(command), NOTICE);
    scroll(&mut s, mouse, &title(command), "NOT acknowledged");
    assert!(!s.screen().contains("[Acknowledge local]"));
    let frame = s.completed_cells_end();
    control(&mut s, mouse, &title(command), "[n New action]", NOTICE);
    s.wait_for_command_count(&f, fresh);
    form_frame(&mut s, frame, &title(command), EDITABLE);
    assert!(!s.screen().contains(BODY));
    populate(&mut s, mouse, command);
    let frame = s.completed_cells_end();
    control(&mut s, mouse, &title(command), "[Preview]", EDITABLE);
    s.wait_for_command_count(&f, previewed);
    form_frame(
        &mut s,
        frame,
        &title(command),
        &["READ-ONLY PREVIEW", "[Confirm action]"],
    );
    s.paste("V18_CONFIRMED_IS_NOT_ACK_POISON");
    assert!(!s.screen().contains("V18_CONFIRMED_IS_NOT_ACK_POISON"));
    control(&mut s, mouse, &title(command), "[Confirm action]", EDITABLE);
    s.wait_for_command_count(&f, steps.len());
    safe(&mut s, "Confirmed; refresh explicitly");
    let db = raw(&f);
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pr_write_attempts WHERE state='confirmed'",
            [],
            |r| r.get::<_, usize>(0)
        )
        .unwrap(),
        2
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pr_write_attempts WHERE state='acknowledged'",
            [],
            |r| r.get::<_, usize>(0)
        )
        .unwrap(),
        0
    );
    let (new_origin,ack):(String,Option<i64>)=db.query_row("SELECT origin_json,parent_ack_attempt_v18 FROM pr_write_conversation_origins_v17 WHERE attempt_id<>?1",[id],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    assert_eq!(ack, None);
    let decoded: Value = serde_json::from_str(&new_origin).unwrap();
    assert_eq!(decoded["seed"]["terminal"]["kind"], "Confirmed");
    assert_eq!(decoded["seed"]["terminal"]["receipt"]["resource_id"], "901");
    assert_eq!(
        db.query_row(
            "SELECT confirmed_json FROM pr_write_attempts WHERE attempt_id=?1",
            [id],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        receipt
    );
    assert_eq!(
        db.query_row(
            "SELECT origin_json FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1",
            [id],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        origin
    );
    assert_eq!(
        db.query_row("SELECT body FROM drafts WHERE resource=?1", [&key], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        body
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM drafts WHERE resource LIKE '%/new-v18/%'",
            [],
            |r| r.get::<_, usize>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='v18_native_cleanup_refused'",
            [],
            |r| r.get::<_, usize>(0)
        )
        .unwrap(),
        0
    );
    refreshed_v19::assert_raw_transition_v19(&f, &stable);
    refreshed_v19::assert_projection_v19(&f, &[(501, OLD), (902, BODY)], &[], 2);
    assert_eq!(git_state(&f), git);
    assert_eq!(
        fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    exact(&s, &f, &steps);
    assert_read_only_git(&f);
    s.quit_and_assert_restored_terminal(mouse);
}
struct ProtectedAttemptV18 {
    id: i64,
    marker: Value,
    origin: Value,
    key: String,
    body: String,
}
fn protected_attempt(f: &Fixture, id: i64) -> ProtectedAttemptV18 {
    let db = raw(f);
    let marker: String = db.query_row(
        "SELECT json_array(attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at) FROM pr_write_attempts WHERE attempt_id=?1",
        [id], |r|r.get(0),
    ).unwrap();
    let origin: String = db.query_row(
        "SELECT json_array(repository_id,pr_id,origin_json,base_key_v18,action_tag_v18,parent_ack_attempt_v18) FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1",
        [id], |r|r.get(0),
    ).unwrap();
    let mut query = db
        .prepare("SELECT resource,body FROM drafts WHERE resource LIKE 'conversation-draft-v17/%'")
        .unwrap();
    let own = query
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .filter(|(_, body)| {
            serde_json::from_str::<Value>(body).unwrap()["publication"]["attempt_id"] == id
        })
        .collect::<Vec<_>>();
    assert_eq!(
        own.len(),
        1,
        "Exactly one actual protected draft for this reference"
    );
    let (key, body) = own.into_iter().next().unwrap();
    let marker: Value = serde_json::from_str(&marker).unwrap();
    assert_eq!(marker[1], HOST);
    assert_eq!(marker[2], "user-id:42");
    assert_eq!(marker[3], "GENUINE_CANONICAL_PR_NODE_1007");
    assert_eq!(marker[6], "conversation-create-v17");
    assert_eq!(marker[10], "uncertain");
    assert!(marker[11].is_null());
    ProtectedAttemptV18 {
        id,
        marker,
        origin: serde_json::from_str(&origin).unwrap(),
        key,
        body,
    }
}
fn assert_protected(f: &Fixture, original: &ProtectedAttemptV18, acknowledged: bool) {
    let now = protected_attempt_row(f, original.id);
    let mut expected = original.marker.clone();
    if acknowledged {
        expected[10] = json!("acknowledged");
    }
    assert_eq!(
        now.0, expected,
        "Only the explicit ACK state transition may change the full raw marker"
    );
    assert_eq!(
        now.1, original.origin,
        "All original JSON/indexed lineage bytes remain inspect-only"
    );
    assert_eq!(
        raw(f)
            .query_row(
                "SELECT body FROM drafts WHERE host=?1 AND login='user-id:42' AND resource=?2",
                [HOST, original.key.as_str()],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        original.body
    );
}
fn protected_attempt_row(f: &Fixture, id: i64) -> (Value, Value) {
    let db = raw(f);
    let marker:String=db.query_row("SELECT json_array(attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at) FROM pr_write_attempts WHERE attempt_id=?1",[id],|r|r.get(0)).unwrap();
    let origin:String=db.query_row("SELECT json_array(repository_id,pr_id,origin_json,base_key_v18,action_tag_v18,parent_ack_attempt_v18) FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1",[id],|r|r.get(0)).unwrap();
    (
        serde_json::from_str(&marker).unwrap(),
        serde_json::from_str(&origin).unwrap(),
    )
}
struct RepeatRoundV18 {
    start: usize,
    page: usize,
    history: usize,
    inspected: usize,
    next_page: usize,
    notice: usize,
    fresh: usize,
    preview: usize,
    end: usize,
}
fn denied_create() -> GhStep {
    let mut denied = receipt(PrCommand::NewComment).remove(0);
    denied.status = 1;
    denied.stdout.clear();
    denied.stderr = b"HTTP403 deliberate independent V18 attempt denied".to_vec();
    denied
}
fn repeat_ack_chain(mouse: bool, personal: bool) {
    // One actual fixture across FIVE launches: attempt1 denied -> ACK1 ->
    // explicit New2 denied -> ACK2 -> explicit New3 succeeds. Both ACKs are
    // fresh provider inspections plus the sole real journal transition.
    let command = PrCommand::NewComment;
    let mut steps = initial_steps(personal);
    let first = steps.len();
    steps.extend(phase(command, true, Some(true)));
    let opened = steps.len();
    steps.extend(phase(command, false, Some(true)));
    let prepared = steps.len();
    steps.extend(phase(command, false, Some(true)));
    steps.push(denied_create());
    let first_end = steps.len();
    let mut rounds = Vec::new();
    for second in [false, true] {
        let start = steps.len();
        steps.extend(initial_steps(personal));
        let page = steps.len();
        steps.extend(phase(command, false, Some(true)));
        let history = steps.len();
        steps.extend(inspect_steps(command));
        let inspected = steps.len();
        steps.extend(initial_steps(personal));
        let next_page = steps.len();
        steps.extend(phase(command, true, Some(true)));
        let notice = steps.len();
        steps.extend(phase(command, true, Some(true)));
        let fresh = steps.len();
        steps.extend(phase(command, false, Some(true)));
        let preview = steps.len();
        steps.extend(phase(command, false, Some(true)));
        if second {
            steps.extend(receipt(command));
            steps.extend(visibility());
        } else {
            steps.push(denied_create());
        }
        rounds.push(RepeatRoundV18 {
            start,
            page,
            history,
            inspected,
            next_page,
            notice,
            fresh,
            preview,
            end: steps.len(),
        });
    }
    let f = resource_configured(&steps);
    let git = git_state(&f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    for uid in [42, 43] {
        Cache::open(&database(&f))
            .unwrap()
            .put_draft(
                &Scope::github(HOST, Some(uid)).unwrap(),
                "v18-independent-chain",
                &format!("CHAIN_PRIVATE_{uid}"),
            )
            .unwrap();
    }
    let mut s = launch(&f, mouse);
    current_page(&mut s, &f, mouse, personal, 0, command, &steps[..first]);
    let original_sql = stable_sql(&f);
    let _ = expected_queue_sql(&f, personal, &original_sql, 1);
    prepare(&mut s, &f, mouse, command, opened, prepared);
    control(&mut s, mouse, &title(command), "[Confirm action]", EDITABLE);
    s.wait_for_command_count(&f, first_end);
    safe(&mut s, "Unknown outcome; inspect");
    let root_id: i64 = raw(&f)
        .query_row(
            "SELECT attempt_id FROM pr_write_attempts WHERE state='uncertain'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut protected = vec![protected_attempt(&f, root_id)];
    exact(&s, &f, &steps[..first_end]);
    assert_eq!(stable_sql(&f), original_sql);
    s.quit_and_assert_restored_terminal(mouse);
    for (round, region) in rounds.iter().enumerate() {
        let mut s = launch(&f, mouse);
        current_page(
            &mut s,
            &f,
            mouse,
            personal,
            region.start,
            command,
            &steps[..region.page],
        );
        let expected = expected_queue_sql(&f, personal, &original_sql, 2 + round as u64 * 2);
        let history_title = format!("Original Conversation history • {FIRST}#7");
        let frame = s.completed_cells_end();
        crate::pr_actions::menu(&mut s, mouse, "Inspect pending result");
        s.wait_for_command_count(&f, region.history);
        form_frame(&mut s, frame, &history_title, HISTORY);
        let frame = s.completed_cells_end();
        control(&mut s, mouse, &history_title, "[Inspect current]", HISTORY);
        s.wait_for_command_count(&f, region.inspected);
        form_frame(&mut s, frame, &history_title, HISTORY);
        scroll(&mut s,mouse,&history_title,"Original created comment identity was never observed; UNKNOWN attribution. No first/newest comment is chosen.");
        assert_eq!(raw(&f).query_row("SELECT generation FROM pr_write_scopes WHERE host=?1 AND principal='user-id:42' AND pr_node='GENUINE_CANONICAL_PR_NODE_1007'",[HOST],|r|r.get::<_,i64>(0)).unwrap(),3+round as i64*4);
        for (at, witness) in protected.iter().enumerate() {
            assert_protected(&f, witness, at < round);
        }
        control(
            &mut s,
            mouse,
            &history_title,
            "[Acknowledge local]",
            HISTORY,
        );
        safe(&mut s, "Local ACK #");
        for witness in &protected {
            assert_protected(&f, witness, true);
        }
        assert_eq!(raw(&f).query_row("SELECT generation FROM pr_write_scopes WHERE host=?1 AND principal='user-id:42' AND pr_node='GENUINE_CANONICAL_PR_NODE_1007'",[HOST],|r|r.get::<_,i64>(0)).unwrap(),4+round as i64*4);
        assert_eq!(stable_sql(&f), expected);
        exact(&s, &f, &steps[..region.inspected]);
        s.quit_and_assert_restored_terminal(mouse);
        let mut s = launch(&f, mouse);
        current_page(
            &mut s,
            &f,
            mouse,
            personal,
            region.inspected,
            command,
            &steps[..region.next_page],
        );
        let expected = expected_queue_sql(&f, personal, &original_sql, 3 + round as u64 * 2);
        let frame = s.completed_cells_end();
        crate::pr_actions::menu(
            &mut s,
            mouse,
            "New comment • eligibility UNKNOWN/server-enforced",
        );
        s.wait_for_command_count(&f, region.notice);
        form_frame(&mut s, frame, &title(command), NOTICE);
        scroll(&mut s, mouse, &title(command), "outcome may remain unknown");
        assert!(!s.screen().contains("[Confirm action]"));
        assert_eq!(
            raw(&f)
                .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                    .get::<_, usize>(0))
                .unwrap(),
            round + 1
        );
        for witness in &protected {
            assert_protected(&f, witness, true);
        }
        let frame = s.completed_cells_end();
        control(&mut s, mouse, &title(command), "[n New action]", NOTICE);
        s.wait_for_command_count(&f, region.fresh);
        form_frame(&mut s, frame, &title(command), EDITABLE);
        assert!(
            !s.screen().contains(BODY),
            "Old protected text is never fresh editable input"
        );
        populate(&mut s, mouse, command);
        let frame = s.completed_cells_end();
        control(&mut s, mouse, &title(command), "[Preview]", EDITABLE);
        s.wait_for_command_count(&f, region.preview);
        form_frame(
            &mut s,
            frame,
            &title(command),
            &["READ-ONLY PREVIEW", "[Confirm action]"],
        );
        s.paste("V18_REPEATED_ACK_PREVIEW_POISON");
        assert!(!s.screen().contains("V18_REPEATED_ACK_PREVIEW_POISON"));
        control(&mut s, mouse, &title(command), "[Confirm action]", EDITABLE);
        s.wait_for_command_count(&f, region.end);
        safe(
            &mut s,
            if round == 0 {
                "Unknown outcome; inspect"
            } else {
                "Confirmed; refresh explicitly"
            },
        );
        let next_id: i64 = raw(&f)
            .query_row("SELECT max(attempt_id) FROM pr_write_attempts", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(next_id > protected.last().unwrap().id);
        let (json,base,ack):(String,String,i64)=raw(&f).query_row("SELECT origin_json,base_key_v18,parent_ack_attempt_v18 FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1",[next_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
        assert_eq!(base, protected[0].key);
        assert_eq!(
            ack, root_id,
            "Root ACK remains fixed through the immediate second ACK"
        );
        let decoded: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            decoded["seed"]["terminal"]["reference"]["attempt_id"],
            protected.last().unwrap().id
        );
        assert_eq!(decoded["seed"]["terminal"]["kind"], "Acknowledged");
        for witness in &protected {
            assert_protected(&f, witness, true);
        }
        assert_eq!(
            raw(&f)
                .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                    .get::<_, usize>(0))
                .unwrap(),
            round + 2
        );
        assert_eq!(raw(&f).query_row("SELECT generation FROM pr_write_scopes WHERE host=?1 AND principal='user-id:42' AND pr_node='GENUINE_CANONICAL_PR_NODE_1007'",[HOST],|r|r.get::<_,i64>(0)).unwrap(),7+round as i64*4);
        if round == 0 {
            protected.push(protected_attempt(&f, next_id));
            assert_ne!(protected[0].key, protected[1].key);
        } else {
            let marker = protected_attempt_row(&f, next_id).0;
            assert_eq!(marker[10], "confirmed");
            let effect: Value = serde_json::from_str(marker[11].as_str().unwrap()).unwrap();
            assert_eq!(effect["resource_id"], "901");
            assert_eq!(
                raw(&f)
                    .query_row(
                        "SELECT count(*) FROM drafts WHERE resource LIKE '%/new-v18/%'",
                        [],
                        |r| r.get::<_, usize>(0)
                    )
                    .unwrap(),
                1,
                "Only inspect-only attempt2 remains; successful attempt3 cleanup is exact"
            );
        }
        if round == 0 {
            assert_eq!(stable_sql(&f), expected);
        } else {
            refreshed_v19::assert_published_transition_v19(&f, &expected, command, 3);
        }
        assert_eq!(git_state(&f), git);
        assert_eq!(
            fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
            config
        );
        exact(&s, &f, &steps[..region.end]);
        assert_read_only_git(&f);
        s.quit_and_assert_restored_terminal(mouse);
    }
}
fn all_six(mouse: bool, personal: bool) {
    for command in SIX {
        if command == PrCommand::NewComment {
            repeat_ack_chain(mouse, personal);
        } else {
            run(command, mouse, personal, ModeV18::Publish);
        }
    }
}
fn bounded(mouse: bool, personal: bool) {
    run(PrCommand::NewComment, mouse, personal, ModeV18::CancelNew);
    run(PrCommand::NewComment, mouse, personal, ModeV18::AccountLoss);
}
#[test]
fn conversation_new_action_v18_catalogue_six_same_action_restart_keyboard() {
    all_six(false, false)
}
#[test]
fn conversation_new_action_v18_catalogue_six_same_action_restart_mouse() {
    all_six(true, false)
}
#[test]
fn conversation_new_action_v18_personal_six_same_action_restart_keyboard() {
    all_six(false, true)
}
#[test]
fn conversation_new_action_v18_personal_six_same_action_restart_mouse() {
    all_six(true, true)
}
#[test]
fn conversation_new_action_v18_catalogue_actual_confirmed_retained_restart_keyboard() {
    retained_confirmed(false, false)
}
#[test]
fn conversation_new_action_v18_catalogue_actual_confirmed_retained_restart_mouse() {
    retained_confirmed(true, false)
}
#[test]
fn conversation_new_action_v18_personal_actual_confirmed_retained_restart_keyboard() {
    retained_confirmed(false, true)
}
#[test]
fn conversation_new_action_v18_personal_actual_confirmed_retained_restart_mouse() {
    retained_confirmed(true, true)
}
#[test]
fn conversation_new_action_v18_catalogue_new_phase_cancel_clear_uid_loss_keyboard() {
    bounded(false, false)
}
#[test]
fn conversation_new_action_v18_catalogue_new_phase_cancel_clear_uid_loss_mouse() {
    bounded(true, false)
}
#[test]
fn conversation_new_action_v18_personal_new_phase_cancel_clear_uid_loss_keyboard() {
    bounded(false, true)
}
#[test]
fn conversation_new_action_v18_personal_new_phase_cancel_clear_uid_loss_mouse() {
    bounded(true, true)
}
