//! Source-only declarations: actual Review browser/detail/Form/Preview with
//! shared public assets, strict offline GH, independent SQL/Git/TTY effects.
use super::*;
use crate::positive_pid_publication_v1 as pid_publication;
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use std::io::Cursor;
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Ready,
    Failed,
    Cancel,
    Privacy,
    Disabled,
    NoColor,
}
fn author(id: u64, login: &str, kind: &str) -> Value {
    json!({"id":id,"login":login,"type":kind,"avatar_url":format!("https://avatars.githubusercontent.com/u/{id}?v=4")})
}
fn actual_review(h: &history::GitImagesV1) -> Value {
    let mut observed = row(h, Effect::Dismiss, Failure::None, false);
    observed["user"] = author(201, "actual-review-author", "User");
    observed
}
fn actual_snapshot(h: &history::GitImagesV1) -> Vec<GhStep> {
    let observed = actual_review(h);
    let mut comments = comments(h, Effect::Dismiss, Failure::None, "RIGHT");
    comments[0]["user"] = author(202, "comment-bot", "Bot");
    let mut deleted = comments[0].clone();
    deleted["id"] = json!(912);
    deleted["node_id"] = json!("ACTUAL_DELETED_COMMENT_912");
    deleted["user"] = json!({"id":null,"login":"ghost","type":"deleted","avatar_url":null});
    deleted["body"] = json!("Actual deleted actor comment");
    comments.as_array_mut().unwrap().push(deleted);
    vec![
        get(
            &format!("repos/{FIRST}/pulls/7/reviews/810"),
            observed.clone(),
        ),
        get(
            &format!("repos/{FIRST}/pulls/7/reviews/810/comments?per_page=100&page=1"),
            comments,
        ),
        get(&format!("repos/{FIRST}/pulls/7/reviews/810"), observed),
    ]
}
fn install_assets(fixture: &Fixture, mode: Mode) {
    for (id, color) in [(201, [255, 0, 0, 255]), (202, [0, 255, 0, 255])] {
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba(color)))
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        fs::write(
            fixture.root.join(format!("review-avatar-{id}.png")),
            bytes.into_inner(),
        )
        .unwrap();
    }
    let effect = if mode == Mode::Failed {
        "exit 22\n".to_owned()
    } else {
        format!(
            "/bin/cat '{}/review-avatar-'\"$id\"'.png'\n",
            fixture.root.display()
        )
    };
    let script=format!("#!/bin/sh\nid=\nfor arg do case \"$arg\" in 'https://avatars.githubusercontent.com/u/201?v=4') id=201;; 'https://avatars.githubusercontent.com/u/202?v=4') id=202;; esac; done\nif [ -z \"$id\" ]; then printf unexpected > '{root}/review-curl-unexpected'; exit 94; fi\nprintf '%s\\n' \"$$\" >> '{root}/review-curl-'\"$id\"'-pid'\nprintf '%s\\n' \"$@\" >> '{root}/review-curl-'\"$id\"'-argv'\n/usr/bin/env | /usr/bin/sed 's/=.*//' >> '{root}/review-curl-'\"$id\"'-env'\nprintf 'complete\\n' > '{root}/review-curl-'\"$id\"'-metadata'\ntries=0\nwhile [ ! -e '{root}/review-asset-release' ] && [ \"$tries\" -lt 600 ]; do /bin/sleep 0.01; tries=$((tries+1)); done\nprintf complete > '{root}/review-curl-'\"$id\"'-complete'\n{effect}",root=fixture.root.display());
    let curl = fixture.root.join("bin/curl");
    // This fixture's parent installs only gh, not a forbidden curl probe.
    // Install one isolated public-asset executable; never fall through to the
    // host curl or manufacture an existing executable to satisfy rename.
    assert!(!curl.exists());
    crate::fixture_shell_publication_v1::install(&curl, script.as_bytes()).unwrap();
}
fn sidecar(fixture: &Fixture, id: u64, suffix: &str) -> std::path::PathBuf {
    fixture.root.join(format!("review-curl-{id}-{suffix}"))
}
fn started(session: &mut Session, fixture: &Fixture, id: u64) -> ProcessIdentity {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Ok(bytes) = fs::read(sidecar(fixture, id, "pid")) {
            if let Some(pid) = pid_publication::complete_positive_pid(&bytes)
                .expect("One complete positive PID, never partial publication")
            {
                if fs::read(sidecar(fixture, id, "metadata"))
                    .is_ok_and(|bytes| bytes == b"complete\n")
                {
                    if let Some(actual) = process_identity(pid).unwrap() {
                        let owned =
                            OwnedPtySession::capture(session.owned_application_pid() as u32)
                                .unwrap();
                        assert!(owned.owns(&actual));
                        assert!(!actual.zombie);
                        return actual;
                    }
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "Actual held curl reaches PID plus full metadata barrier: {}",
            session.screen()
        );
        session.poll_output(Duration::from_millis(10));
    }
}
fn reaped(session: &mut Session, original: &ProcessIdentity) {
    let deadline = Instant::now() + Duration::from_secs(3);
    let owner = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
    loop {
        let Some(now) = process_identity(original.pid).unwrap() else {
            return;
        };
        assert!(owner.owns(&now));
        assert_eq!(
            (now.pid, now.uid, now.birth, now.session),
            (original.pid, original.uid, original.birth, original.session)
        );
        if now.zombie && now.parent == std::process::id() as i32 {
            let mut status = 0;
            // SAFETY: actual adopted positive exact UID/birth/SID child only.
            assert_eq!(
                unsafe { libc::waitpid(now.pid, &mut status, libc::WNOHANG) },
                now.pid
            );
            assert!(process_identity(now.pid).unwrap().is_none());
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Same original public asset is reaped"
        );
        session.poll_output(Duration::from_millis(10));
    }
}
fn rgb(session: &Session, color: (u8, u8, u8)) -> bool {
    let mut parser = vt100::Parser::new(40, 120, 0);
    parser.process(&session.transcript);
    (0..40).any(|y| {
        (0..120).any(|x| {
            parser.screen().cell(y, x).is_some_and(|cell| {
                cell.contents() == "▀"
                    && cell.fgcolor() == vt100::Color::Rgb(color.0, color.1, color.2)
                    && cell.bgcolor() == cell.fgcolor()
            })
        })
    })
}
fn pixels(session: &mut Session, color: (u8, u8, u8)) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !rgb(session, color) {
        assert!(
            Instant::now() < deadline,
            "Actual decoded Review avatar RGB: {}",
            session.screen()
        );
        session.poll_output(Duration::from_millis(10));
    }
}
fn curl_effects(fixture: &Fixture, ids: &[u64]) {
    assert!(!fixture.root.join("review-curl-unexpected").exists());
    for id in [201, 202] {
        if !ids.contains(&id) {
            assert!(
                !sidecar(fixture, id, "pid").exists(),
                "Hidden/disabled actor {id} must not request"
            );
            continue;
        }
        assert_eq!(
            fs::read(sidecar(fixture, id, "metadata")).unwrap(),
            b"complete\n"
        );
        let pids = fs::read(sidecar(fixture, id, "pid")).unwrap();
        assert!(
            pid_publication::complete_positive_pid(&pids)
                .unwrap()
                .is_some(),
            "Exactly one actual request, including resize/Form cache reuse"
        );
        let argv = fs::read_to_string(sidecar(fixture, id, "argv")).unwrap();
        assert!(argv.starts_with("-q\n--fail\n--silent\n--show-error\n"));
        assert!(argv.contains("--proto\n=https\n--max-redirs\n0"));
        assert!(argv.contains("--max-filesize\n1048576"));
        assert_eq!(argv.lines().filter(|arg| *arg == "--url").count(), 1);
        assert!(argv.ends_with(&format!(
            "--url\nhttps://avatars.githubusercontent.com/u/{id}?v=4\n"
        )));
        assert!(!argv.contains("Authorization"));
        let env = fs::read_to_string(sidecar(fixture, id, "env")).unwrap();
        for forbidden in [
            "GH_TOKEN",
            "GITHUB_TOKEN",
            "GH_ENTERPRISE_TOKEN",
            "GITHUB_ENTERPRISE_TOKEN",
            "GH_HOST",
            "GH_CONFIG_DIR",
            "CURL_HOME",
            "HTTP_PROXY",
            "HTTPS_PROXY",
        ] {
            assert!(!env.lines().any(|key| key == forbidden));
        }
    }
}
fn journey(mouse: bool, mode: Mode) {
    let (fixture, h, mut steps) = fixture_and_steps(|_| {});
    steps.truncate(steps.len() - 17);
    install_assets(&fixture, mode);
    let actual = actual_review(&h);
    steps.extend(source_open(&h, json!([actual.clone()])));
    let details = matches!(
        mode,
        Mode::Ready | Mode::Privacy | Mode::Disabled | Mode::NoColor
    );
    if details {
        steps.extend(opening(&h));
        steps.extend(actual_snapshot(&h));
        let mut close = closing(&h);
        if mode == Mode::Privacy {
            let last = close.len() - 1;
            close[last] = get("user", json!({"id":43,"login":"actual-other-account"}));
        }
        steps.extend(close);
    }
    if mode == Mode::Ready {
        steps.extend(source_open(&h, json!([actual]))); // exact selected-action draft, not batch key
        steps.extend(opening(&h));
        steps.extend(actual_snapshot(&h));
        steps.extend(closing(&h));
    }
    fixture.scripted_gh(&steps);
    foreign_draft(&fixture);
    let config_path = fixture.root.join("config/ghtui/config.toml");
    let mut config = Config::load(&config_path).unwrap();
    config.avatars = mode != Mode::Disabled;
    config.refresh_seconds = 3600;
    config.save(&config_path).unwrap();
    let config_bytes = fs::read(&config_path).unwrap();
    drop(ghtui::pr_write_journal::PrWriteJournal::open(&database(&fixture)).unwrap());
    drop(ghtui::actions_write_journal::ActionsWriteJournal::open(&database(&fixture)).unwrap());
    let git_before = git_state(&fixture);
    let drafts_before = stable_drafts(&fixture);
    let trace = fixture.root.join("catalogue-git-trace.jsonl");
    let mut env = vec![
        ("GIT_TRACE2_EVENT", trace.to_str().unwrap()),
        ("COLORTERM", "truecolor"),
    ];
    if mode == Mode::NoColor {
        env.push(("NO_COLOR", "1"));
    }
    let mut session = Session::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &fixture,
        120,
        40,
        &env,
    );
    session.wait_for_screen("Recent commits");
    session.assert_focused_view("Repository");
    files_selected(&mut session, &fixture, mouse);
    let git_journal = git_commands(&fixture);
    let sql_before = sql_state(&fixture);
    curl_effects(&fixture, &[]);
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "[F10 Reviews]", b"\x1b[21~");
    browser_ready(&mut session, before, "#810");
    ready(
        &mut session,
        &[
            "@actual-review-author",
            "UID Some(201)",
            "requester UID Some(42)",
        ],
    );
    if matches!(mode, Mode::Disabled | Mode::NoColor) {
        ready(&mut session, &["[AA]"]);
        curl_effects(&fixture, &[]);
        assert!(!rgb(&session, (255, 0, 0)));
    } else {
        let held = started(&mut session, &fixture, 201);
        curl_effects(&fixture, &[201]);
        if matches!(mode, Mode::Cancel | Mode::Privacy) {
            if mode == Mode::Cancel {
                let before = session.completed_cursorless_end();
                choose(&mut session, mouse, "[Esc Close]", b"\x1b");
                session.wait_for_new_cursorless_condition(
                    before,
                    |frame| {
                        !frame.contents().contains("Scoped reviews")
                            && !frame.contents().contains("Actual author/private body")
                    },
                    "Exact review Close hides its own private body",
                    Duration::from_secs(8),
                );
            } else {
                let before = session.completed_cursorless_end();
                choose(&mut session, mouse, "[R Resume selected]", b"R");
                session.wait_for_new_cursorless_condition(
                    before,
                    |frame| {
                        let text = frame.contents();
                        !text.contains("Scoped reviews")
                            && !text.contains("Actual server review body")
                            && !text.contains("Actual existing server pending comment")
                            && !text.contains("old-no-newline")
                    },
                    "Fresh final numeric account loss hides original private browser/images",
                    Duration::from_secs(8),
                );
            }
            reaped(&mut session, &held);
        } else {
            let before = session.completed_cursorless_end();
            session.resize_owned_control_block_v3(80, 24, "Scoped reviews", &REVIEW_CONTROLS_V12);
            session.wait_for_new_cursorless_condition(
                before,
                |frame| {
                    frame.contents().contains("Scoped reviews")
                        && frame.contents().contains("[Esc Close]")
                },
                "Actual review resize preserves exact owned surface",
                Duration::from_secs(8),
            );
            session.resize_owned_control_block_v3(120, 40, "Scoped reviews", &REVIEW_CONTROLS_V12);
            ready(&mut session, &["Scoped reviews", "[R Resume selected]"]);
            curl_effects(&fixture, &[201]);
            fs::write(
                fixture.root.join("review-asset-release"),
                b"Explicit image release",
            )
            .unwrap();
            if mode == Mode::Ready {
                pixels(&mut session, (255, 0, 0));
            } else {
                session
                    .try_wait_for_condition(
                        |_| sidecar(&fixture, 201, "complete").exists(),
                        "Actual failed public fetch completed",
                        Duration::from_secs(3),
                    )
                    .unwrap();
                ready(&mut session, &["[AA]"]);
                assert!(!rgb(&session, (255, 0, 0)));
            }
        }
    }
    if details && mode != Mode::Privacy {
        let before = session.completed_cursorless_end();
        choose(&mut session, mouse, "[R Resume selected]", b"R");
        browser_ready(
            &mut session,
            before,
            "Actual existing server pending comment",
        );
        ready(
            &mut session,
            &[
                "@comment-bot",
                "[bot]",
                "ghost",
                "[deleted]",
                "UID Some(202)",
            ],
        );
        if mode == Mode::Ready {
            pixels(&mut session, (0, 255, 0));
            curl_effects(&fixture, &[201, 202]);
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
            ready(
                &mut session,
                &[
                    "Observed review author @actual-review-author",
                    "UID Some(201)",
                ],
            );
            pixels(&mut session, (255, 0, 0));
            session.paste("Avatar display preview only");
            let before = session.completed_cursorless_end();
            choose(&mut session, mouse, "[Preview]", b"\x10");
            session.wait_for_new_cursorless_condition(
                before,
                |frame| {
                    frame
                        .contents()
                        .contains("READ-ONLY PREVIEW • Confirm action")
                        && frame.contents().contains("Current H")
                },
                "Actual fresh review Preview prepared with observed author map",
                Duration::from_secs(8),
            );
            let before = session.completed_cursorless_end();
            session.paste("AVATAR_PREVIEW_POISON_MUST_NOT_EDIT");
            session.wait_for_new_cursorless_condition(
                before,
                |frame| {
                    frame
                        .contents()
                        .contains("READ-ONLY PREVIEW • Confirm action")
                        && !frame
                            .contents()
                            .contains("AVATAR_PREVIEW_POISON_MUST_NOT_EDIT")
                },
                "Prepared Preview ignores poison paste and retains actual target",
                Duration::from_secs(8),
            );
            let before = session.completed_cursorless_end();
            if mouse {
                session.wheel(40, 15, true);
            } else {
                session.send_key(b"\x1b[6~");
            }
            session.wait_for_new_cursorless_condition(
                before,
                |frame| {
                    frame.contents().contains("Observed comment #911")
                        && frame.contents().contains("@comment-bot")
                },
                "Actual original comment header after pure preview scroll",
                Duration::from_secs(8),
            );
            pixels(&mut session, (0, 255, 0));
            curl_effects(&fixture, &[201, 202]);
            kept(&mut session, mouse);
        } else {
            curl_effects(&fixture, &[]);
            assert!(!rgb(&session, (255, 0, 0)) && !rgb(&session, (0, 255, 0)));
        }
    }
    if matches!(mode, Mode::Failed | Mode::Disabled | Mode::NoColor) {
        choose(&mut session, mouse, "[Esc Close]", b"\x1b");
        session.wait_for_overlay_close("Scoped reviews");
    }
    exact_commands(&session, &fixture, &steps, 0);
    assert_eq!(stable_drafts(&fixture), drafts_before);
    assert_eq!(git_state(&fixture), git_before);
    assert_eq!(git_commands(&fixture), git_journal);
    assert_eq!(fs::read(&config_path).unwrap(), config_bytes);
    assert_eq!(
        raw(&fixture)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    if mode != Mode::Ready {
        assert_eq!(sql_state(&fixture), sql_before);
    }
    session.quit_and_assert_restored_terminal(mouse);
    exact_commands(&session, &fixture, &steps, 0);
}
macro_rules! pair {
    ($key:ident,$mouse:ident,$mode:ident) => {
        #[test]
        fn $key() {
            journey(false, Mode::$mode)
        }
        #[test]
        fn $mouse() {
            journey(true, Mode::$mode)
        }
    };
}
pair!(
    pending_reviews_v2_avatars_v7_keyboard_ready_browser_detail_form_preview,
    pending_reviews_v2_avatars_v7_mouse_ready_browser_detail_form_preview,
    Ready
);
pair!(
    pending_reviews_v2_avatars_v7_keyboard_failed_resize_no_retry,
    pending_reviews_v2_avatars_v7_mouse_failed_resize_no_retry,
    Failed
);
pair!(
    pending_reviews_v2_avatars_v7_keyboard_held_close_reaped,
    pending_reviews_v2_avatars_v7_mouse_held_close_reaped,
    Cancel
);
pair!(
    pending_reviews_v2_avatars_v7_keyboard_visible_final_uid_loss,
    pending_reviews_v2_avatars_v7_mouse_visible_final_uid_loss,
    Privacy
);
pair!(
    pending_reviews_v2_avatars_v7_keyboard_disabled_initials_http_zero,
    pending_reviews_v2_avatars_v7_mouse_disabled_initials_http_zero,
    Disabled
);
pair!(
    pending_reviews_v2_avatars_v7_keyboard_nocolor_initials_http_zero,
    pending_reviews_v2_avatars_v7_mouse_nocolor_initials_http_zero,
    NoColor
);
