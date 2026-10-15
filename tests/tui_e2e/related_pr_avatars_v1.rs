//! Actual modal + public curl bytes/argv/PID/cache; strict offline GH only.
use super::*;
#[path = "../support/positive_pid_publication_v1.rs"]
mod pid_publication_v1;
use crate::support::owned_pty_processes_v1::{process_identity, OwnedPtySession, ProcessIdentity};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use std::io::Cursor;
#[path = "timeline_actor_related_v1.rs"]
mod timeline_actor_v1;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Ready,
    PartialPid,
    Failed,
    Cancel,
    Privacy,
    Disabled,
    NoColor,
}
struct Asset {
    release: std::path::PathBuf,
}
fn asset(fixture: &Fixture, mode: Mode) -> Asset {
    for (id, color) in [
        (201, [255, 0, 0, 255]),
        (202, [0, 255, 0, 255]),
        (303, [0, 0, 255, 255]),
    ] {
        let mut image = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba(color)))
            .write_to(&mut image, ImageFormat::Png)
            .unwrap();
        fs::write(
            fixture.root.join(format!("related-{id}.png")),
            image.into_inner(),
        )
        .unwrap();
    }
    let release = fixture.root.join("related-asset-release");
    let effect = if mode == Mode::Failed {
        "exit 22\n".to_owned()
    } else {
        format!(
            "/bin/cat '{}/related-'\"$id\"'.png'\n",
            fixture.root.display()
        )
    };
    let publication = if mode == Mode::PartialPid {
        format!("if [ \"$id\" = 201 ]; then\nprintf '%s' \"$$\" >> '{root}/related-curl-'\"$id\"'-pid'\nprintf pending > '{root}/related-pid-pending'\npublish_tries=0\nwhile [ ! -f '{root}/related-pid-publish' ] && [ \"$publish_tries\" -lt 300 ]; do /bin/sleep 0.01; publish_tries=$((publish_tries + 1)); done\nprintf '\\n' >> '{root}/related-curl-'\"$id\"'-pid'\nelse\nprintf '%s\\n' \"$$\" >> '{root}/related-curl-'\"$id\"'-pid'\nfi\n",root=fixture.root.display())
    } else {
        format!(
            "printf '%s\\n' \"$$\" >> '{}/related-curl-'\"$id\"'-pid'\n",
            fixture.root.display()
        )
    };
    let script=format!("#!/bin/sh\nid=\nfor arg do case \"$arg\" in 'https://avatars.githubusercontent.com/u/201?v=4') id=201;; 'https://avatars.githubusercontent.com/u/202?v=4') id=202;; 'https://avatars.githubusercontent.com/u/303?v=4') id=303;; esac; done\nif [ -z \"$id\" ]; then printf unexpected > '{root}/related-curl-unexpected'; exit 94; fi\n{publication}printf '%s\\n' \"$@\" >> '{root}/related-curl-'\"$id\"'-argv'\n/usr/bin/env | /usr/bin/sed 's/=.*//' >> '{root}/related-curl-'\"$id\"'-env'\ntries=0\nwhile [ ! -e '{gate}' ] && [ \"$tries\" -lt 600 ]; do /bin/sleep 0.01; tries=$((tries + 1)); done\nprintf complete > '{root}/related-curl-'\"$id\"'-complete'\n{effect}",root=fixture.root.display(),gate=release.display());
    let program = fixture.root.join("bin/curl");
    // Preserve the parent's deliberate HTTP-forbidden probe; never normalize
    // its executable or accidentally reopen it while the TUI is running.
    fs::rename(&program, fixture.root.join("original-forbidden-role-curl")).unwrap();
    crate::fixture_shell_publication_v1::install(&program, script.as_bytes()).unwrap();
    Asset { release }
}
fn asset_path(fixture: &Fixture, id: u64, suffix: &str) -> std::path::PathBuf {
    fixture.root.join(format!("related-curl-{id}-{suffix}"))
}
fn started(session: &mut Session, fixture: &Fixture, id: u64) -> ProcessIdentity {
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        if let Ok(bytes) = fs::read(asset_path(fixture, id, "pid")) {
            if let Some(pid) = pid_publication_v1::complete_positive_pid(&bytes)
                .expect("Actual fixture PID publication must be one positive complete record")
            {
                if let Some(actual) = process_identity(pid).unwrap() {
                    let owner =
                        OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
                    assert!(owner.owns(&actual));
                    assert!(!actual.zombie);
                    return actual;
                }
            }
        }
        assert!(
            Instant::now() < end,
            "Actual held public curl child starts: {}",
            session.screen()
        );
        session.poll_output(Duration::from_millis(10));
    }
}
fn reaped(session: &mut Session, original: &ProcessIdentity) {
    let owner = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        let Some(actual) = process_identity(original.pid).unwrap() else {
            return;
        };
        assert!(owner.owns(&actual));
        assert_eq!(
            (actual.pid, actual.uid, actual.session, actual.birth),
            (original.pid, original.uid, original.session, original.birth)
        );
        if actual.zombie && actual.parent == std::process::id() as i32 {
            let mut status = 0;
            // SAFETY: exact positive owned UID/birth/SID, actually adopted.
            assert_eq!(
                unsafe { libc::waitpid(actual.pid, &mut status, libc::WNOHANG) },
                actual.pid
            );
            assert!(libc::WIFEXITED(status) || libc::WIFSIGNALED(status));
            assert!(process_identity(actual.pid).unwrap().is_none());
            return;
        }
        assert!(
            Instant::now() < end,
            "Original public image child is reaped"
        );
        session.poll_output(Duration::from_millis(10));
    }
}
fn rgb(session: &Session, color: (u8, u8, u8)) -> bool {
    let mut parser = vt100::Parser::new(40, 120, 0);
    parser.process(&session.transcript);
    (0..40).any(|y| {
        (0..120).any(|x| {
            parser.screen().cell(y, x).is_some_and(|c| {
                c.contents() == "▀"
                    && c.fgcolor() == vt100::Color::Rgb(color.0, color.1, color.2)
                    && c.bgcolor() == c.fgcolor()
            })
        })
    })
}
fn pixels(session: &mut Session, color: (u8, u8, u8)) {
    let end = Instant::now() + Duration::from_secs(3);
    while !rgb(session, color) {
        assert!(
            Instant::now() < end,
            "Actual decoded Related avatar pixels: {}",
            session.screen()
        );
        session.poll_output(Duration::from_millis(10));
    }
}
fn effects(fixture: &Fixture, ids: &[u64]) {
    assert!(!fixture.root.join("related-curl-unexpected").exists());
    assert!(!fixture.root.join("role-curl-pid").exists());
    for id in [201, 202, 303] {
        if !ids.contains(&id) {
            assert!(
                !asset_path(fixture, id, "pid").exists(),
                "Hidden actor {id} must not dispatch"
            );
            continue;
        }
        let args = fs::read_to_string(asset_path(fixture, id, "argv")).unwrap();
        assert_eq!(args.lines().filter(|a| *a == "--url").count(), 1);
        assert!(args.starts_with("-q\n--fail\n--silent\n--show-error\n"));
        assert!(args.contains("--proto\n=https\n--max-redirs\n0"));
        assert!(args.contains("--max-filesize\n1048576"));
        assert!(args.ends_with(&format!(
            "--url\nhttps://avatars.githubusercontent.com/u/{id}?v=4\n"
        )));
        assert!(!args.contains("Authorization"));
        let env = fs::read_to_string(asset_path(fixture, id, "env")).unwrap();
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
            assert!(!env.lines().any(|e| e == forbidden));
        }
    }
}
fn journey(mouse: bool, mode: Mode) {
    let source = source("sha1");
    let fixture = &source.fixture;
    let asset = asset(fixture, mode);
    let mut steps = protocol(&source.original, Case::ReplacedOrigin);
    steps.extend(discovery(&source.original, 42, 42, false));
    if matches!(mode, Mode::Ready | Mode::PartialPid) {
        let mut open = opening(&source.original);
        // Preserve genuine bounded --paginate/--slurp argv; replace only the
        // actual observed body, never turn a strict fixture into a loose read.
        open[6].stdout = json!([[{"id":77,"node_id":"COMMENT_77","body":"Observed related comment green actor","user":actor(202,"comment-writer"),"created_at":"2026-10-14T00:00:00Z","updated_at":"2026-10-14T00:00:00Z"}]]).to_string().into_bytes();
        open[10].stdout = json!([[{"id":303,"event":"commented","actor":actor(303,"timeline-writer"),"created_at":"2026-10-14T00:00:00Z","body":"Observed hidden timeline blue actor"}]]).to_string().into_bytes();
        steps.extend(open);
    } else if mode == Mode::Privacy {
        steps.extend(discovery(&source.original, 43, 43, false));
    }
    fixture.scripted_gh(&steps);
    let config_path = fixture.root.join("config/ghtui/config.toml");
    Config {
        host: HOST.into(),
        refresh_seconds: 3600,
        avatars: mode != Mode::Disabled,
        ..Default::default()
    }
    .save(&config_path)
    .unwrap();
    let config = fs::read(&config_path).unwrap();
    drop(ghtui::pr_write_journal::PrWriteJournal::open(&database(fixture)).unwrap());
    drop(ghtui::actions_write_journal::ActionsWriteJournal::open(&database(fixture)).unwrap());
    let before = state(fixture);
    let trace_path = fixture.root.join("git-role-trace.jsonl");
    let mut env = vec![("GIT_TRACE2_EVENT", trace_path.to_str().unwrap())];
    if mode == Mode::NoColor {
        env.push(("NO_COLOR", "1"));
    }
    let mut session = Session::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        fixture,
        120,
        40,
        &env,
    );
    saved_ready(
        &mut session,
        &source.head,
        &source.head,
        &["Recent commits"],
    );
    choose(&mut session, mouse, "[g Graph]", b"g");
    select_original(&mut session, mouse, &source.original);
    choose(&mut session, mouse, "[a Author]", b"a");
    saved_ready(
        &mut session,
        &source.original,
        &source.head,
        &["Actor profile — Git author"],
    );
    choose_repository(&mut session, fixture, mouse);
    session.resize(120, 40);
    let original_sql = sql(fixture);
    let original_trace = trace(fixture);
    effects(fixture, &[]);
    choose(&mut session, mouse, "[P Related PRs]", b"P");
    completed(&mut session, &[TITLE, PRIVATE, "@host-writer"], &[]);
    strict(&session, fixture, 15);
    let mut seen = vec![];
    if matches!(mode, Mode::Disabled | Mode::NoColor) {
        completed(&mut session, &[TITLE, "[OW]", "@host-writer"], &["▀"]);
        effects(fixture, &[]);
    } else {
        if mode == Mode::PartialPid {
            let end = Instant::now() + Duration::from_secs(3);
            while !fixture.root.join("related-pid-pending").exists() {
                assert!(
                    Instant::now() < end,
                    "Actual curl producer reaches incomplete PID publication"
                );
                session.poll_output(Duration::from_millis(10));
            }
            let unpublished = fs::read(asset_path(fixture, 201, "pid")).unwrap();
            assert!(!unpublished.is_empty() && !unpublished.ends_with(b"\n"));
            assert_eq!(
                pid_publication_v1::complete_positive_pid(&unpublished).unwrap(),
                None,
                "No kernel process lookup/ownership from unpublished digits"
            );
            completed(&mut session, &[TITLE, "[OW]", "@host-writer"], &["▀"]);
            session.resize_owned_modal(80, 24, TITLE, &["[Esc Back]", "[r Discover]"]);
            session.resize_owned_modal(120, 40, TITLE, &["[Esc Back]", "[r Discover]"]);
            assert_eq!(
                pid_publication_v1::complete_positive_pid(
                    &fs::read(asset_path(fixture, 201, "pid")).unwrap()
                )
                .unwrap(),
                None
            );
            strict(&session, fixture, 15);
            fs::write(
                fixture.root.join("related-pid-publish"),
                b"explicit complete publication",
            )
            .unwrap();
        }
        let child = started(&mut session, fixture, 201);
        seen.push(201);
        completed(&mut session, &[TITLE, "[OW]", "@host-writer"], &["▀"]);
        // Explicit held worker plus resize, not arbitrary readiness sleeps.
        session.resize_owned_modal(80, 24, TITLE, &["[Esc Back]", "[r Discover]"]);
        session.resize_owned_modal(120, 40, TITLE, &["[Esc Back]", "[r Discover]"]);
        let live = process_identity(child.pid)
            .unwrap()
            .expect("Resize retains original image worker");
        assert_eq!(
            (live.uid, live.session, live.birth),
            (child.uid, child.session, child.birth)
        );
        if matches!(mode, Mode::Cancel | Mode::Privacy) {
            if mode == Mode::Cancel {
                choose(&mut session, mouse, "[Esc Back]", b"\x1b");
            } else {
                choose(&mut session, mouse, "[r Discover]", b"r");
                completed(
                    &mut session,
                    &[TITLE, "Original related PR read unavailable"],
                    &[PRIVATE, "@host-writer", "▀"],
                );
                strict(&session, fixture, 17);
            }
            reaped(&mut session, &child);
            assert!(!asset_path(fixture, 201, "complete").exists());
            assert!(!fixture.root.join("config/ghtui/avatars").exists());
        } else {
            fs::write(&asset.release, b"explicit release").unwrap();
            if mode == Mode::Failed {
                reaped(&mut session, &child);
                completed(&mut session, &[TITLE, "[OW]", "@host-writer"], &["▀"]);
                session.resize_owned_modal(80, 24, TITLE, &["[Esc Back]", "[r Discover]"]);
                session.resize_owned_modal(120, 40, TITLE, &["[Esc Back]", "[r Discover]"]);
            } else {
                pixels(&mut session, (255, 0, 0));
                choose(&mut session, mouse, "[O Open selected PR]", b"O");
                completed(
                    &mut session,
                    &[TITLE, "Head bbbbb", "PRIVATE_RELATED_BODY_ORIGINAL"],
                    &[],
                );
                pixels(&mut session, (255, 0, 0));
                effects(fixture, &[201]);
                choose(&mut session, mouse, "[c Conversation]", b"c");
                completed(
                    &mut session,
                    &[
                        TITLE,
                        "Observed related comment green actor",
                        "@comment-writer",
                    ],
                    &[],
                );
                pixels(&mut session, (0, 255, 0));
                seen.push(202);
                effects(fixture, &seen);
                choose(&mut session, mouse, "[t Timeline]", b"t");
                completed(
                    &mut session,
                    &[
                        TITLE,
                        "Observed hidden timeline blue actor",
                        "@timeline-writer",
                    ],
                    &[],
                );
                pixels(&mut session, (0, 0, 255));
                seen.push(303);
                effects(fixture, &seen);
                strict(&session, fixture, 31);
                choose(&mut session, mouse, "[Esc Back]", b"\x1b");
                completed(
                    &mut session,
                    &[TITLE, PRIVATE],
                    &["Observed hidden timeline blue actor"],
                );
                for entry in fs::read_dir(fixture.root.join("config/ghtui/avatars")).unwrap() {
                    let record: Value =
                        serde_json::from_slice(&fs::read(entry.unwrap().path()).unwrap()).unwrap();
                    let key = record["key"].as_str().unwrap();
                    assert_eq!(key.split('\0').next(), Some("github.invalid/user-id:42"));
                    assert!(!key.contains("user-id:43"));
                }
            }
        }
    }
    if mode != Mode::Cancel {
        choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    }
    completed(
        &mut session,
        &[
            "Actor profile — Git author",
            &format!("Saved: {}", source.original),
        ],
        &[TITLE, PRIVATE, "▀"],
    );
    effects(fixture, &seen);
    assert_eq!(state(fixture), before);
    assert_eq!(fs::read(&config_path).unwrap(), config);
    assert_eq!(sql(fixture), original_sql);
    assert_eq!(
        trace(fixture).len(),
        original_trace.len()
            + if matches!(mode, Mode::Ready | Mode::PartialPid | Mode::Privacy) {
                4
            } else {
                2
            }
    );
    assert_read_only_trace(fixture);
    let expected = if matches!(mode, Mode::Ready | Mode::PartialPid) {
        31
    } else if mode == Mode::Privacy {
        17
    } else {
        15
    };
    strict(&session, fixture, expected);
    session.quit_and_assert_restored_terminal(mouse);
    strict(&session, fixture, expected);
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
    related_pr_avatars_keyboard_ready_visible_tabs_exact_public_pixels,
    related_pr_avatars_mouse_ready_visible_tabs_exact_public_pixels,
    Ready
);
pair!(
    related_pr_avatars_keyboard_failed_original_no_resize_retry,
    related_pr_avatars_mouse_failed_original_no_resize_retry,
    Failed
);
pair!(
    related_pr_avatars_keyboard_held_cancel_reaps_original_curl,
    related_pr_avatars_mouse_held_cancel_reaps_original_curl,
    Cancel
);
pair!(
    related_pr_avatars_keyboard_account_loss_drains_assets_private_hidden,
    related_pr_avatars_mouse_account_loss_drains_assets_private_hidden,
    Privacy
);
pair!(
    related_pr_avatars_keyboard_disabled_initials_http_zero,
    related_pr_avatars_mouse_disabled_initials_http_zero,
    Disabled
);
pair!(
    related_pr_avatars_keyboard_no_color_initials_http_zero,
    related_pr_avatars_mouse_no_color_initials_http_zero,
    NoColor
);
pair!(
    related_pr_avatars_keyboard_partial_pid_waits_original_complete_publication,
    related_pr_avatars_mouse_partial_pid_waits_original_complete_publication,
    PartialPid
);
