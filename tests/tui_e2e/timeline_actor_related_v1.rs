//! Actual original closed Related source, existing public image cache and
//! pure timeline refusal; provider/principal failures never retain bodies.
use super::*;
use crate::github::timeline_actor_v1::observed;

#[derive(Clone, Copy, PartialEq, Eq)]
enum CaseV1 {
    Roles,
    Cap,
    Privacy,
}
fn journey(mouse: bool, case: CaseV1) {
    let source = source("sha1");
    let fixture = &source.fixture;
    let asset = asset(fixture, Mode::Ready);
    let mut steps = protocol(&source.original, Case::ReplacedOrigin);
    let mut discovery_steps = discovery(&source.original, 42, 42, false);
    let mut open = opening(&source.original);
    if case == CaseV1::Cap {
        let mut summary = pr();
        summary["body"] = json!((0..60)
            .map(|i| format!("Retained Summary body {i:03}"))
            .collect::<Vec<_>>()
            .join("\n"));
        discovery_steps[4].stdout = json!([summary.clone()]).to_string().into_bytes();
        open[4].stdout = json!([summary.clone()]).to_string().into_bytes();
        for index in [5, 11, 12] {
            open[index].stdout = summary.to_string().into_bytes();
        }
        open[10].stdout=json!([[{"event":"commented","user":actor(202,"comment-writer"),"actor":actor(303,"event-generator[bot]"),"body":"x\n".repeat(70_000)}]]).to_string().into_bytes();
    } else {
        open[10].stdout = json!([observed::events()]).to_string().into_bytes();
    }
    steps.extend(discovery_steps);
    steps.extend(open);
    if case == CaseV1::Privacy {
        steps.extend(discovery(&source.original, 43, 43, false));
    }
    fixture.scripted_gh(&steps);
    let config_path = fixture.root.join("config/ghtui/config.toml");
    Config {
        host: HOST.into(),
        refresh_seconds: 3600,
        avatars: true,
        ..Default::default()
    }
    .save(&config_path)
    .unwrap();
    let config = fs::read(&config_path).unwrap();
    drop(ghtui::pr_write_journal::PrWriteJournal::open(&database(fixture)).unwrap());
    drop(ghtui::actions_write_journal::ActionsWriteJournal::open(&database(fixture)).unwrap());
    let original_git = state(fixture);
    let trace_path = fixture.root.join("git-role-trace.jsonl");
    let mut session = Session::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        fixture,
        120,
        40,
        &[("GIT_TRACE2_EVENT", trace_path.to_str().unwrap())],
    );
    saved_ready(
        &mut session,
        &source.head,
        &source.head,
        &["Recent commits"],
    );
    choose(&mut session, mouse, "[g Graph]", b"g");
    select_original(&mut session, mouse, &source.original);
    choose_key_bar(&mut session, mouse, "[a Author]");
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
    let child = started(&mut session, fixture, 201);
    assert!(!child.zombie);
    completed(&mut session, &[TITLE, "[OW]"], &["▀"]);
    effects(fixture, &[201]);
    fs::write(&asset.release, b"explicit image release").unwrap();
    pixels(&mut session, (255, 0, 0));
    choose(&mut session, mouse, "[O Open selected PR]", b"O");
    completed(&mut session, &[TITLE, "Head bbbbb"], &[]);
    strict(&session, fixture, 31);
    pixels(&mut session, (255, 0, 0));
    effects(fixture, &[201]);
    let gh_before = session.inspect_command_journal(fixture);
    let trace_before = trace(fixture);
    if case == CaseV1::Cap {
        completed(&mut session, &[TITLE, "Retained Summary body 000"], &[]);
        if mouse {
            let (x, y) = session.locate("Retained Summary body 000").unwrap();
            session.wheel(x, y, true);
        } else {
            session.send_key(b"\x1b[6~");
        }
        completed(
            &mut session,
            &[TITLE, "Retained Summary body 025"],
            &["Head bbbbb"],
        );
        let retained = session.screen();
        let retained_line = retained
            .lines()
            .find(|line| line.contains("Retained Summary body 025"))
            .unwrap()
            .to_owned();
        let first_body = retained
            .lines()
            .find_map(|line| {
                let begin = line.find("Retained Summary body ")?;
                line.get(begin..begin + "Retained Summary body ".len() + 3)
            })
            .expect("Actual scrolled Summary contains original body rows")
            .to_owned();
        choose(&mut session, mouse, "[t Timeline]", b"t");
        completed(
            &mut session,
            &[TITLE, "Timeline display limit reached", &first_body],
            &["event generator", "comment author"],
        );
        assert!(session.screen().contains(&retained_line));
        session.resize_owned_modal(80, 24, TITLE, &["[Esc Back]", "[r Discover]"]);
        completed(
            &mut session,
            &[TITLE, "Timeline display limit reached", &first_body],
            &[],
        );
        session.resize_owned_modal(120, 40, TITLE, &["[Esc Back]", "[r Discover]"]);
        completed(&mut session, &[TITLE, "Retained Summary body 025"], &[]);
        effects(fixture, &[201]);
    } else {
        choose(&mut session, mouse, "[t Timeline]", b"t");
        session.wait_for_cursorless_condition(|f|{let s=f.contents(); s.contains(TITLE)&&observed::EXPECTED.iter().all(|e|s.contains(e))&&observed::ABSENT.iter().all(|e|!s.contains(e))},"Completed closed Related timeline retains distinct user/generator/date/unknown semantic roles",Duration::from_secs(8));
        pixels(&mut session, (0, 255, 0));
        pixels(&mut session, (0, 0, 255));
        effects(fixture, &[201, 202, 303]);
        session.resize_owned_modal(80, 24, TITLE, &["[Esc Back]", "[r Discover]"]);
        completed(
            &mut session,
            &[TITLE, "comment author", "event generator"],
            observed::ABSENT,
        );
        session.resize_owned_modal(120, 40, TITLE, &["[Esc Back]", "[r Discover]"]);
        completed(&mut session, &[TITLE, "review author"], observed::ABSENT);
        if case == CaseV1::Privacy {
            choose(&mut session, mouse, "[r Discover]", b"r");
            completed(
                &mut session,
                &[TITLE, "Original related PR read unavailable"],
                &[
                    "Exact timeline comment body",
                    "@comment-writer",
                    "@event-generator",
                    PRIVATE,
                    "▀",
                ],
            );
            strict(&session, fixture, 33);
        }
    }
    if case != CaseV1::Privacy {
        assert_eq!(session.inspect_command_journal(fixture), gh_before);
        assert_eq!(trace(fixture), trace_before);
    }
    assert_eq!(sql(fixture), original_sql);
    choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    if case != CaseV1::Privacy {
        completed(
            &mut session,
            &[TITLE, PRIVATE],
            &["Exact timeline comment body", "Retained Summary body"],
        );
        choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    }
    completed(
        &mut session,
        &[
            "Actor profile — Git author",
            &format!("Saved: {}", source.original),
        ],
        &[TITLE, PRIVATE, "Exact timeline comment body", "▀"],
    );
    effects(
        fixture,
        if case == CaseV1::Cap {
            &[201]
        } else {
            &[201, 202, 303]
        },
    );
    assert_eq!(state(fixture), original_git);
    assert_eq!(fs::read(&config_path).unwrap(), config);
    assert_eq!(sql(fixture), original_sql);
    assert_eq!(
        trace(fixture).len(),
        original_trace.len() + if case == CaseV1::Privacy { 6 } else { 4 }
    );
    assert_read_only_trace(fixture);
    strict(&session, fixture, steps.len());
    session.quit_and_assert_restored_terminal(mouse);
    strict(&session, fixture, steps.len());
    assert_eq!(state(fixture), original_git);
    assert_eq!(sql(fixture), original_sql);
}
#[test]
fn timeline_actor_v1_related_keyboard_closed_roles_public_pixels_dedup_resize_back() {
    journey(false, CaseV1::Roles)
}
#[test]
fn timeline_actor_v1_related_mouse_closed_roles_public_pixels_dedup_resize_back() {
    journey(true, CaseV1::Roles)
}
#[test]
fn timeline_actor_v1_related_keyboard_display_limit_retains_original_summary_viewport_http_zero() {
    journey(false, CaseV1::Cap)
}
#[test]
fn timeline_actor_v1_related_mouse_display_limit_retains_original_summary_viewport_http_zero() {
    journey(true, CaseV1::Cap)
}
#[test]
fn timeline_actor_v1_related_keyboard_account_loss_hides_original_timeline_and_drains_images() {
    journey(false, CaseV1::Privacy)
}
#[test]
fn timeline_actor_v1_related_mouse_account_loss_hides_original_timeline_and_drains_images() {
    journey(true, CaseV1::Privacy)
}
