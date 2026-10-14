//! SOURCE-ONLY genuine binary journeys, child of identity::git_actor_labels_v1.
//! ABI/protocol requires unapplied native hooks; six declarations are UNRUN.
use super::*;
#[path = "related_pr_v1.rs"]
mod related_pr_v1;
use crate::support::GhStep;
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use std::{io::Cursor, os::unix::fs::PermissionsExt};

const HOST: &str = "github.invalid";
const CHOOSER: &str = "Repositories • explicit discovery";
const PURPOSE: &str = "Choose repository for saved commit";
const DETAIL: &str = "Saved commit host report • read-only";
const SOURCE_DIALOG: &str = "Repository discovery source";
const ROW: &str = "example/original";
#[derive(Clone, Copy, PartialEq, Eq)]
enum Case {
    Ready,
    NullMonochrome,
    PrincipalLoss,
    Cancel,
    Denied,
    Recreated,
    UnknownPull,
    ReplacedOrigin,
}
fn get(endpoint: &str, value: Value) -> GhStep {
    GhStep::read(
        &["api", endpoint, "--hostname", HOST, "--method", "GET"],
        &value.to_string(),
    )
}
fn principal(uid: u64) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
        get("user", json!({"id":uid,"login":"octocat"})),
    ]
}
fn owner() -> Value {
    json!({"id":71,"node_id":"OWNER_local_actor_71","login":"example","type":"Organization"})
}
fn repo() -> Value {
    json!({"id":101,"node_id":"REPO_local_actor_101","full_name":ROW,"owner":owner(),"private":true,"archived":false,"permissions":{"pull":true},"description":"Selected explicit repository, no local mapping assertion"})
}
fn actor(id: u64, name: &str) -> Value {
    json!({"id":id,"node_id":format!("ACTOR_local_{id}"),"login":name,"name":format!("Observed {name}"),"type":"User","avatar_url":format!("https://avatars.githubusercontent.com/u/{id}?v=4"),"url":format!("https://github.invalid/api/v3/users/{name}")})
}
fn protocol(sha: &str, case: Case) -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get("orgs/example", owner()));
    steps.push(get(
        "orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page=1",
        json!([repo()]),
    ));
    steps.push(get("orgs/example", owner()));
    steps.extend(principal(42));
    assert_eq!(steps.len(), 7);
    if case == Case::ReplacedOrigin {
        return steps;
    }
    steps.extend(principal(42));
    let mut current = repo();
    if case == Case::Recreated {
        current["id"] = json!(102);
        current["node_id"] = json!("REPO_recreated_102");
    }
    if case == Case::UnknownPull {
        current["permissions"] = Value::Null;
    }
    let mut selected = get("repos/example/original", current);
    if case == Case::Denied {
        selected.status = 1;
        selected.stdout.clear();
        selected.stderr = b"gh: Forbidden (HTTP 403)".to_vec();
    }
    steps.push(selected);
    if matches!(case, Case::Denied | Case::Recreated | Case::UnknownPull) {
        steps.extend(principal(42));
        assert_eq!(steps.len(), 12);
        return steps;
    }
    let raw = json!({"sha":sha,"url":format!("https://github.invalid/api/v3/repos/example/original/commits/{sha}"),"html_url":format!("https://github.invalid/example/original/commit/{sha}"),"commit":{"url":format!("https://github.invalid/api/v3/repos/example/original/git/commits/{sha}"),"author":{"name":"","email":"","date":"2023-11-14T22:13:20Z"},"committer":{"name":"Exact Recorder","email":"record@example.invalid","date":"2023-11-14T22:13:30Z"}},"author":if case==Case::NullMonochrome{Value::Null}else{actor(201,"host-writer")},"committer":actor(202,"host-recorder")});
    let mut commit = get(
        &format!("repos/example/original/commits/{sha}?per_page=1"),
        raw,
    );
    if case == Case::Cancel {
        commit.delay_ms = 6000;
    }
    steps.push(commit);
    steps.push(get("repos/example/original", repo()));
    steps.extend(principal(if case == Case::PrincipalLoss { 43 } else { 42 }));
    assert_eq!(steps.len(), 14);
    steps
}
fn asset_program(fixture: &Fixture) {
    for (id, color) in [(201, [255, 0, 0, 255]), (202, [0, 255, 0, 255])] {
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba(color)))
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        fs::write(
            fixture.root.join(format!("role-{id}.png")),
            bytes.into_inner(),
        )
        .unwrap();
    }
    let script=format!("#!/bin/sh\nasset=\ntag=\nfor arg do case \"$arg\" in 'https://avatars.githubusercontent.com/u/201?v=4') asset='{root}/role-201.png'; tag=201;; 'https://avatars.githubusercontent.com/u/202?v=4') asset='{root}/role-202.png'; tag=202;; esac; done\nif [ -z \"$asset\" ]; then printf unexpected > '{root}/actor-curl-unexpected'; exit 94; fi\nprintf '%s\\n' \"$$\" >> '{root}/actor-curl-'\"$tag\"'-pids'\nprintf '%s\\n' \"$@\" >> '{root}/actor-curl-'\"$tag\"'-argv'\n/usr/bin/env | /usr/bin/sed 's/=.*//' >> '{root}/actor-curl-'\"$tag\"'-env'\n/bin/cat \"$asset\"\n",root=fixture.root.display());
    let program = fixture.root.join("bin/curl");
    fs::write(&program, script).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
}
fn sql(fixture: &Fixture) -> Vec<String> {
    let connection = rusqlite::Connection::open(database(fixture)).unwrap();
    let mut result = Vec::new();
    for statement in ["SELECT json_array(host,login,name,payload) FROM saved_views ORDER BY host,login,name",
        "SELECT json_array(host,login,resource,body) FROM drafts ORDER BY host,login,resource",
        "SELECT json_array(host,login,source,generation,active) FROM sync_reservations ORDER BY host,login,source",
        "SELECT json_array(host,login,kind,id,payload,complete) FROM records ORDER BY host,login,kind,id",
        "SELECT json_array(host,login,source,generation,complete) FROM checkpoints ORDER BY host,login,source",
        "SELECT json_array(attempt_id,state) FROM pr_write_attempts ORDER BY attempt_id",
        "SELECT json_array(attempt_id,state) FROM actions_write_attempts ORDER BY attempt_id"] {
        let mut query=connection.prepare(statement).unwrap(); result.extend(query.query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap));
    }
    result
}
fn gh(session: &Session, fixture: &Fixture) -> Vec<Value> {
    session
        .inspect_command_journal(fixture)
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
fn assert_gh(session: &Session, fixture: &Fixture, count: usize) {
    let rows = gh(session, fixture);
    assert_eq!(rows.len(), count);
    assert!(rows
        .iter()
        .all(|r| r["matched"] == true && r["stdin_hex"] == ""));
    assert!(!rows.iter().any(|r| r["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a == "--paginate" || a == "--slurp" || a == "graphql")));
}
fn wait_gh(session: &mut Session, fixture: &Fixture, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        session.poll_output(Duration::from_millis(10));
        if gh(session, fixture).len() == count {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Actual strict gh ordinal {count}; {}",
            session.screen()
        );
    }
}
fn overlay(session: &mut Session, title: &str, labels: &[&str]) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(10));
        if session
            .visible_overlay_body(title)
            .is_some_and(|body| labels.iter().all(|label| body.contains(label)))
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Actual owned overlay {title} with {labels:?}; {}",
            session.screen()
        );
    }
}
fn choose_repository(session: &mut Session, fixture: &Fixture, mouse: bool) {
    choose(session, mouse, "[R Choose repository]", b"R");
    ready(session, &[CHOOSER, PURPOSE, "[s Source]", "[Esc Close]"]);
    assert_gh(session, fixture, 0);
    choose(session, mouse, "[s Source]", b"s");
    overlay(
        session,
        SOURCE_DIALOG,
        &["[o Organization]", "Owner", "[Enter Discover]"],
    );
    choose(session, mouse, "[o Organization]", b"o");
    if mouse {
        session.click_visible_action("Owner");
    } else {
        session.send_key(b"\t");
    }
    session.send_key(b"\x01");
    session.paste("example");
    choose(session, mouse, "[Enter Discover]", b"\r");
    session.wait_for_overlay_close(SOURCE_DIALOG);
    ready(
        session,
        &[
            CHOOSER,
            PURPOSE,
            ROW,
            "Repository ID: 101",
            "[Enter Use repository]",
        ],
    );
    assert_gh(session, fixture, 7);
    for forbidden in [
        "[w Save set]",
        "[a Activate set]",
        "[p Pin]",
        "[Space Select]",
        "[Open My PRs]",
        "[Open Actions]",
    ] {
        assert!(
            !session.screen().contains(forbidden),
            "No chooser escape: {forbidden}"
        );
    }
    session.resize_owned_modal(
        if mouse { 120 } else { 80 },
        if mouse { 40 } else { 24 },
        CHOOSER,
        &["[q Quit]", "[Esc Close]"],
    );
    ready(session, &[PURPOSE, ROW, "[Enter Use repository]"]);
    if mouse {
        session.click_visible_action("◆ repository · example/original");
    } else {
        session.send_key(b"\t");
    }
    let original_sql = sql(fixture);
    let original_git = trace(fixture);
    let original_gh = gh(session, fixture);
    choose(session, mouse, "[Enter Use repository]", b"\r");
    session.wait_for_overlay_close(CHOOSER);
    session.wait_for_main_frame();
    ready(
        session,
        &[
            "Actor profile — Git author",
            "[V Verify host report]",
            "Repository selected as navigation intent",
        ],
    );
    assert_eq!(sql(fixture), original_sql);
    assert_eq!(trace(fixture), original_git);
    assert_eq!(gh(session, fixture), original_gh);
}
fn current_pixels(session: &Session, cols: u16, rows: u16) -> bool {
    let mut parser = vt100::Parser::new(rows, cols, 0);
    parser.process(&session.transcript);
    let mut red = false;
    let mut green = false;
    for y in 0..rows {
        for x in 0..cols {
            if let Some(c) = parser.screen().cell(y, x) {
                if c.contents() == "▀" && c.fgcolor() == c.bgcolor() {
                    red |= c.fgcolor() == vt100::Color::Rgb(255, 0, 0);
                    green |= c.fgcolor() == vt100::Color::Rgb(0, 255, 0);
                }
            }
        }
    }
    red && green
}
fn public_effects(session: &mut Session, fixture: &Fixture, expected: usize) {
    assert!(!fixture.root.join("actor-curl-unexpected").exists());
    let args = [201, 202]
        .into_iter()
        .map(|id| {
            fs::read_to_string(fixture.root.join(format!("actor-curl-{id}-argv")))
                .unwrap_or_default()
        })
        .collect::<String>();
    assert_eq!(
        args.lines().filter(|line| *line == "--url").count(),
        expected
    );
    if expected > 0 {
        assert!(args.contains("--proto\n=https\n--max-redirs\n0"));
        assert!(args.contains("--max-filesize\n1048576"));
        let names = [201, 202]
            .into_iter()
            .map(|id| {
                fs::read_to_string(fixture.root.join(format!("actor-curl-{id}-env")))
                    .unwrap_or_default()
            })
            .collect::<String>();
        for forbidden in [
            "GH_TOKEN",
            "GITHUB_TOKEN",
            "GH_ENTERPRISE_TOKEN",
            "GH_HOST",
            "GH_CONFIG_DIR",
            "CURL_HOME",
            "HTTPS_PROXY",
            "HTTP_PROXY",
        ] {
            assert!(!names.lines().any(|name| name == forbidden));
        }
        let pids = [201, 202]
            .into_iter()
            .map(|id| {
                fs::read_to_string(fixture.root.join(format!("actor-curl-{id}-pids")))
                    .unwrap_or_default()
            })
            .collect::<String>();
        for line in pids.lines() {
            let pid = line.parse::<i32>().unwrap();
            assert!(pid > 0);
            let until = Instant::now() + Duration::from_secs(3);
            loop {
                if unsafe { libc::kill(pid, 0) } == -1 {
                    assert_eq!(
                        std::io::Error::last_os_error().raw_os_error(),
                        Some(libc::ESRCH)
                    );
                    break;
                }
                assert!(Instant::now() < until);
                session.poll_output(Duration::from_millis(10));
            }
        }
    } else {
        for id in [201, 202] {
            assert!(!fixture.root.join(format!("actor-curl-{id}-pids")).exists());
        }
    }
}
fn journey(mouse: bool, case: Case) {
    let formats: &[&str] = if case == Case::Ready {
        &["sha1", "sha256"]
    } else {
        &["sha1"]
    };
    for format in formats {
        let source = source(format);
        let fixture = &source.fixture;
        let actual = if *format == "sha256" {
            Case::NullMonochrome
        } else {
            case
        };
        fixture.scripted_gh(&protocol(&source.original, actual));
        asset_program(fixture);
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
        let mut before = state(fixture);
        let intent =
            json!({"kind":"Original saved local navigation intent","saved":source.original});
        {
            let cache = Cache::open(&database(fixture)).unwrap();
            let scope = Scope::github(HOST, Some(42)).unwrap();
            cache
                .put_draft(&scope, DRAFT, "Original42 unsent Git role text")
                .unwrap();
            cache.save_view(&scope, INTENT, &intent).unwrap();
        }
        // A local-only launch does not need either publication journal. Create
        // their real shared schemas explicitly for the independent no-attempt
        // SQL oracle; absence of a table must never be accepted as zero writes.
        drop(ghtui::pr_write_journal::PrWriteJournal::open(&database(fixture)).unwrap());
        drop(ghtui::actions_write_journal::ActionsWriteJournal::open(&database(fixture)).unwrap());
        let trace_path = fixture.root.join("git-role-trace.jsonl");
        let mut env = vec![("GIT_TRACE2_EVENT", trace_path.to_str().unwrap())];
        if actual == Case::NullMonochrome {
            env.push(if mouse {
                ("TERM", "dumb")
            } else {
                ("NO_COLOR", "1")
            });
        }
        let (mut cols, mut rows) = if mouse { (80, 24) } else { (120, 40) };
        let mut session = Session::launch_binary(
            Path::new(env!("CARGO_BIN_EXE_ghtui")),
            fixture,
            cols,
            rows,
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
            &[
                "Actor profile — Git author",
                "author unknown name [email unavailable]",
            ],
        );
        choose_repository(&mut session, fixture, mouse);
        (cols, rows) = if mouse { (120, 40) } else { (80, 24) };
        let mut retained_original = None;
        if actual == Case::ReplacedOrigin {
            let root = fixture.repo();
            let preserved = fixture.root.join("original-source-kept");
            fs::rename(&root, &preserved).unwrap();
            let output = Command::new("git")
                .args(["clone", "--quiet", "--no-hardlinks"])
                .arg(&preserved)
                .arg(&root)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "Controlled local fixture clone: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            fs::write(
                root.join("role-scratch.txt"),
                b"replacement untracked bytes\n",
            )
            .unwrap();
            assert_eq!(
                fixture
                    .git(&["rev-parse", "refs/tags/original-git-role"])
                    .trim(),
                source.original
            );
            before = state(fixture);
            retained_original = Some((
                preserved.clone(),
                fs::read(preserved.join(".git/index")).unwrap(),
                fs::read(preserved.join("readme")).unwrap(),
                fs::read(preserved.join("role-scratch.txt")).unwrap(),
                fs::read(preserved.join(".git/config")).unwrap(),
            ));
        }
        let git_config = fs::read(fixture.repo().join(".git/config")).unwrap();
        let saved_sql = sql(fixture);
        let trace_before = trace(fixture);
        let gh_before = gh(&session, fixture);
        assert_eq!(gh_before.len(), 7);
        choose(&mut session, mouse, "[V Verify host report]", b"V");
        if actual == Case::Cancel {
            wait_gh(&mut session, fixture, 11);
            let child_pid = gh(&session, fixture)[10]["pid"].as_i64().unwrap() as i32;
            assert!(child_pid > 0);
            ready(&mut session, &["Esc Cancel"]);
            choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
            ready(&mut session, &["Cancelled", "Actor profile — Git author"]);
            session.wait_for_main_frame();
            choose(&mut session, mouse, "[c Committer]", b"c");
            saved_ready(
                &mut session,
                &source.original,
                &source.head,
                &["Actor profile — Git committer"],
            );
            assert!(!session.screen().contains("@host-writer"));
            assert!(!session.screen().contains("@host-recorder"));
            assert_gh(&session, fixture, 11);
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if unsafe { libc::kill(child_pid, 0) } == -1 {
                    assert_eq!(
                        std::io::Error::last_os_error().raw_os_error(),
                        Some(libc::ESRCH)
                    );
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "Actual canceled original gh child must be reaped"
                );
                session.poll_output(Duration::from_millis(10));
            }
            public_effects(&mut session, fixture, 0);
        } else if matches!(
            actual,
            Case::Denied | Case::Recreated | Case::UnknownPull | Case::ReplacedOrigin
        ) {
            let expected = if actual == Case::ReplacedOrigin {
                7
            } else {
                12
            };
            wait_gh(&mut session, fixture, expected);
            session.try_wait_for_condition(|screen| (screen.contains("Host report unavailable") || screen.contains("Private host report retired"))
                && screen.contains("local Git"), "Actual completed unavailable/private-retired host report, not transient verifying footer", Duration::from_secs(8)).unwrap();
            session.wait_for_main_frame();
            assert!(!session.screen().contains("@host-writer"));
            assert!(!session.screen().contains("@host-recorder"));
            assert!(!session.screen().contains('▀'));
            choose(&mut session, mouse, "[c Committer]", b"c");
            saved_ready(
                &mut session,
                &source.original,
                &source.head,
                &["committer Exact Recorder"],
            );
            assert_gh(&session, fixture, expected);
            public_effects(&mut session, fixture, 0);
        } else if actual == Case::PrincipalLoss {
            wait_gh(&mut session, fixture, 14);
            ready(&mut session, &["local Git remains"]);
            session.wait_for_main_frame();
            assert!(!session.screen().contains("@host-writer"));
            assert!(!session.screen().contains("@host-recorder"));
            assert!(!session.screen().contains('▀'));
            choose(&mut session, mouse, "[c Committer]", b"c");
            saved_ready(
                &mut session,
                &source.original,
                &source.head,
                &["committer Exact Recorder"],
            );
            assert_gh(&session, fixture, 14);
            public_effects(&mut session, fixture, 0);
        } else {
            wait_gh(&mut session, fixture, 14);
            ready(&mut session, &["Original observed host report"]);
            if actual == Case::Ready {
                let until = Instant::now() + Duration::from_secs(4);
                while !current_pixels(&session, cols, rows) {
                    assert!(
                        Instant::now() < until,
                        "Two actual role pixels in current frame: {}",
                        session.screen()
                    );
                    session.poll_output(Duration::from_millis(10));
                }
            }
            let pure_git = trace(fixture);
            let pure_gh = gh(&session, fixture);
            choose(&mut session, mouse, "[F7 Report details]", b"\x1b[18~");
            overlay(
                &mut session,
                DETAIL,
                &[
                    "GitHub reports this commit in selected repository",
                    "@host-recorder",
                    "Not commit-signature verification",
                ],
            );
            if actual == Case::Ready {
                assert!(session.screen().contains("@host-writer"));
            } else {
                assert!(!session.screen().contains("@host-writer"));
                assert!(session.screen().contains("Author host account unavailable"));
            }
            if mouse {
                let p = session.locate("Observed repository:").unwrap();
                session.wheel(p.0, p.1, true);
                let deadline = Instant::now() + Duration::from_secs(8);
                loop {
                    session.poll_output(Duration::from_millis(10));
                    if session.visible_overlay_body(DETAIL).is_some_and(|body| {
                        !body.contains("GitHub reports this commit in selected repository")
                            && body.contains("Not commit-signature verification")
                    }) {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "Actual hovered report wheel changes the visible rows"
                    );
                }
                session.wheel(p.0, p.1, false);
                overlay(
                    &mut session,
                    DETAIL,
                    &[
                        "GitHub reports this commit in selected repository",
                        "[Esc Back]",
                    ],
                );
            } else {
                session.focus_visible_action("[↓ Later]");
                let (column, row) = session.locate("[↓ Later]").unwrap();
                assert!(
                    session.cell_underlined(row, column),
                    "Actual overlay button focus is visible"
                );
                session.send_key(b"\r");
                let deadline = Instant::now() + Duration::from_secs(8);
                loop {
                    session.poll_output(Duration::from_millis(10));
                    if session.visible_overlay_body(DETAIL).is_some_and(|body| {
                        !body.contains("GitHub reports this commit in selected repository")
                            && body.contains("Not commit-signature verification")
                    }) {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "Actual focused Later action changes the visible rows"
                    );
                }
                session.focus_visible_action("[↑ Earlier]");
                session.send_key(b"\r");
                overlay(
                    &mut session,
                    DETAIL,
                    &[
                        "GitHub reports this commit in selected repository",
                        "[Esc Back]",
                    ],
                );
            }
            choose(&mut session, mouse, "[Esc Back]", b"\x1b");
            session.wait_for_overlay_close(DETAIL);
            session.wait_for_main_frame();
            choose(&mut session, mouse, "[c Committer]", b"c");
            saved_ready(
                &mut session,
                &source.original,
                &source.head,
                &["Actor profile — Git committer"],
            );
            session.resize(if mouse { 80 } else { 120 }, if mouse { 24 } else { 40 });
            assert_eq!(trace(fixture), pure_git);
            assert_eq!(gh(&session, fixture), pure_gh);
            assert_eq!(sql(fixture), saved_sql);
            public_effects(
                &mut session,
                fixture,
                if actual == Case::Ready { 2 } else { 0 },
            );
            choose(&mut session, mouse, "[X Clear host report]", b"X");
            session.try_wait_for_condition(|screen| {
                screen.contains("Host report cleared")
                    && screen.contains("committer Exact Recorder")
                    && !screen.contains('▀')
                    && !screen.contains("@host-recorder")
                    && !screen.contains("@host-writer")
            }, "Actual Clear completion removes both host roles and pixels, preserving local committer", Duration::from_secs(8)).unwrap();
            assert!(!session.screen().contains('▀'));
            assert!(!session.screen().contains("@host-recorder"));
            assert_gh(&session, fixture, 14);
        }
        let after = trace(fixture);
        let new = &after[trace_before.len()..];
        assert_eq!(
            new.len(),
            if actual == Case::ReplacedOrigin { 0 } else { 2 },
            "Only measured raw original two Git children, no rediscovery/checkout/ref resolution"
        );
        for entry in new {
            let argv = entry["argv"].as_array().unwrap();
            assert!(argv.iter().any(|a| a == "cat-file"));
            assert!(!argv.iter().any(|a| a == "rev-parse"));
        }
        assert_eq!(state(fixture), before);
        assert_eq!(
            fs::read(fixture.repo().join(".git/config")).unwrap(),
            git_config
        );
        assert_eq!(fs::read(&config_path).unwrap(), config);
        assert_eq!(sql(fixture), saved_sql);
        {
            let cache = Cache::open(&database(fixture)).unwrap();
            let original = Scope::github(HOST, Some(42)).unwrap();
            assert_eq!(
                cache.draft(&original, DRAFT).unwrap().as_deref(),
                Some("Original42 unsent Git role text")
            );
            assert_eq!(
                cache.saved_views(&original).unwrap(),
                vec![(INTENT.into(), intent.clone())]
            );
            let other = Scope::github(HOST, Some(43)).unwrap();
            assert!(cache.list(&other, None).unwrap().is_empty());
            assert!(cache.saved_views(&other).unwrap().is_empty());
            assert!(cache.draft(&other, DRAFT).unwrap().is_none());
        }
        session.quit_and_assert_restored_terminal(mouse);
        assert_eq!(state(fixture), before);
        assert_eq!(fs::read(&config_path).unwrap(), config);
        assert_read_only_trace(fixture);
        if let Some((root, index, work, scratch, configuration)) = retained_original {
            assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
            assert_eq!(fs::read(root.join("readme")).unwrap(), work);
            assert_eq!(fs::read(root.join("role-scratch.txt")).unwrap(), scratch);
            assert_eq!(fs::read(root.join(".git/config")).unwrap(), configuration);
        }
        assert_eq!(
            fixture.git(&["cat-file", "commit", &source.original]),
            source.original_object
        );
        assert_eq!(
            fixture.git(&["cat-file", "commit", &source.head]),
            source.head_object
        );
    }
}
#[test]
fn git_actor_association_native_keyboard_exact_saved_roles_pixels_null_and_no_color() {
    journey(false, Case::Ready);
}
#[test]
fn git_actor_association_native_mouse_exact_saved_roles_pixels_null_and_no_color() {
    journey(true, Case::Ready);
}
#[test]
fn git_actor_association_native_keyboard_final_uid_loss_hides_reports_preserves_local() {
    journey(false, Case::PrincipalLoss);
}
#[test]
fn git_actor_association_native_mouse_final_uid_loss_hides_reports_preserves_local() {
    journey(true, Case::PrincipalLoss);
}
#[test]
fn git_actor_association_native_keyboard_delayed_read_same_token_cancel_no_late_effect() {
    journey(false, Case::Cancel);
}
#[test]
fn git_actor_association_native_mouse_delayed_read_same_token_cancel_no_late_effect() {
    journey(true, Case::Cancel);
}
#[test]
fn git_actor_association_native_keyboard_current_denied_recreated_and_unknown_are_not_reports() {
    for case in [Case::Denied, Case::Recreated, Case::UnknownPull] {
        journey(false, case);
    }
}
#[test]
fn git_actor_association_native_mouse_current_denied_recreated_and_unknown_are_not_reports() {
    for case in [Case::Denied, Case::Recreated, Case::UnknownPull] {
        journey(true, case);
    }
}
#[test]
fn git_actor_association_native_keyboard_same_path_same_sha_replacement_stops_before_any_raw_or_gh()
{
    journey(false, Case::ReplacedOrigin);
}
#[test]
fn git_actor_association_native_mouse_same_path_same_sha_replacement_stops_before_any_raw_or_gh() {
    journey(true, Case::ReplacedOrigin);
}
