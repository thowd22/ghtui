//! Actual Native PR adapter into the ONE external pane/engine. TWO genuinely
//! changed files, one verified selected proof; no whole-PR scope fabrication.
use super::*;
use crate::native_external_tools_v4::{ExternalToolsV4, PagerV4, RendererV4};
use crate::support::owned_pty_processes_v1::ProcessIdentity;
use std::ffi::OsStr;
use std::path::PathBuf;
const CHOOSER: &str = "Optional external diff • read-only";
const PANE: &str = "Optional external pager • host-owned read-only PTY";
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn fixture() -> (Fixture, history::GitImagesV1, Vec<GhStep>) {
    let mut extra = None;
    let (fixture, history, mut steps) = fixture_with_history(
        |repo| {
            let (h, whole, other) = history::GitImagesV1::create_with_extra_file(
                repo,
                false,
                history::OLD,
                history::NEW,
            );
            extra = Some((whole, other));
            h
        },
        |_| {},
    );
    let (whole, other) = extra.unwrap();
    for step in &mut steps {
        if step.args.first().is_some_and(|arg| arg == "pr") {
            step.stdout = whole.clone();
            continue;
        }
        let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) else {
            continue;
        };
        if value.get("head").is_some() && value.get("base").is_some() {
            value["changed_files"] = json!(2);
            value["additions"] = json!(history.additions + 1);
            value["deletions"] = json!(history.deletions + 1);
        }
        if step
            .args
            .get(1)
            .is_some_and(|path| path.contains("/pulls/7/files"))
        {
            value = if step.args.iter().any(|a| a == "--slurp") {
                json!([[history.file(), other.clone()]])
            } else {
                json!([history.file(), other.clone()])
            };
        }
        step.stdout = serde_json::to_vec(&value).unwrap();
    }
    fixture.scripted_gh(&steps);
    (fixture, history, steps)
}
fn external(session: &mut Session, text: &str) {
    session.wait_for_cursorless_condition(
        |frame| {
            crate::support::overlay_body(frame, PANE).is_some_and(|body| {
                body.contains(text) && body.contains("[Esc Return]") && body.contains("[^C Cancel]")
            })
        },
        "current genuine host-owned PR external pane",
        Duration::from_secs(8),
    );
}
fn input(session: &mut Session, mouse: bool, title: &str, label: &str, key: &[u8]) {
    let point = session.wait_for_owned_completed_action(title, label, &[]);
    if mouse {
        session.click(point.0, point.1);
    } else {
        session.send_key(key);
    }
}
fn chosen(session: &mut Session, mouse: bool, structural: bool) {
    act(session, mouse, NATIVE, "[E External]", b"E");
    session.wait_for_cursorless_condition(
        |frame| {
            crate::support::overlay_body(frame, CHOOSER).is_some_and(|body| {
                body.contains("ONE VERIFIED SELECTED PR FILE")
                    && body.contains("not the whole PR")
                    && body.contains("Lifecycle ready")
            })
        },
        "explicit selected-file PR scope before optional Launch",
        Duration::from_secs(8),
    );
    input(
        session,
        mouse,
        CHOOSER,
        if structural {
            "[t difftastic]"
        } else {
            "[d delta]"
        },
        if structural { b"t" } else { b"d" },
    );
    session.wait_for_cursorless_condition(
        |frame| {
            crate::support::overlay_body(frame, CHOOSER).is_some_and(|body| {
                body.contains(if structural {
                    "Selected: difftastic"
                } else {
                    "Selected: delta"
                })
            })
        },
        "actual current remote optional tool choice",
        Duration::from_secs(8),
    );
    input(session, mouse, CHOOSER, "[Enter Launch]", b"\r");
}
fn identity(session: &Session, row: &Value) -> ProcessIdentity {
    assert_eq!(row["stdin"], true);
    assert_eq!(row["stdout"], true);
    assert_eq!(row["stderr"], true);
    let flags = row["rawDescription"]
        .as_str()
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>();
    assert!(flags
        .iter()
        .any(|flag| flag.trim_end_matches(';') == "-icanon"));
    assert!(flags
        .iter()
        .any(|flag| flag.trim_end_matches(';') == "-echo"));
    let pid = i32::try_from(row["pid"].as_i64().unwrap()).unwrap();
    let found = process_identity(pid).unwrap().unwrap();
    assert_eq!(found.session, pid);
    assert_eq!(found.parent, session.owned_application_pid());
    assert_ne!(found.session, unsafe {
        libc::getsid(session.owned_application_pid())
    });
    found
}
fn cleanup(session: &mut Session, owned: &ProcessIdentity, directory: &Path) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(current) = process_identity(owned.pid).unwrap() {
            assert_eq!(
                (current.uid, current.session, current.birth),
                (owned.uid, owned.session, owned.birth)
            );
            if current.zombie && current.parent == std::process::id() as i32 {
                let mut status = 0;
                assert_eq!(
                    unsafe { libc::waitpid(current.pid, &mut status, libc::WNOHANG) },
                    current.pid
                );
                assert!(libc::WIFEXITED(status) || libc::WIFSIGNALED(status));
            }
        }
        if process_identity(owned.pid).unwrap().is_none() && !directory.exists() {
            return;
        }
        session.poll_output(Duration::from_millis(1));
        assert!(
            Instant::now() < until,
            "actual PR child and its private capture must both be gone"
        );
    }
}
fn actual_selected(session: &Session) -> String {
    session
        .completed_cursorless_screen()
        .unwrap()
        .contents()
        .lines()
        .find_map(|row| {
            row.strip_prefix("│>")
                .map(|row| row.trim_end_matches('│').trim().to_owned())
        })
        .expect("a genuinely drawn original selected row")
}
#[test]
fn native_external_diff_keyboard_pr_selected_section_verified_m_h_full_sides_no_transport_or_write()
{
    remote(false);
}
#[test]
fn native_external_diff_mouse_pr_selected_section_verified_m_h_full_sides_no_transport_or_write() {
    remote(true);
}
fn remote(mouse: bool) {
    let (fixture, history, steps) = fixture();
    let tools = ExternalToolsV4::install(&fixture, RendererV4::Ready, PagerV4::Hostile);
    let git = readonly_git(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let trace = fixture.root.join("catalogue-git-trace.jsonl");
    let mut env = vec![
        (OsStr::new("PATH"), tools.path()),
        (OsStr::new("GIT_TRACE2_EVENT"), trace.as_os_str()),
    ];
    if mouse {
        env.push((OsStr::new("NO_COLOR"), OsStr::new("1")));
    }
    let mut session = Session::launch_binary_os_env(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &fixture,
        if mouse { 80 } else { 120 },
        if mouse { 24 } else { 40 },
        &env,
    );
    session.wait_for_screen("Recent commits");
    files_selected(&mut session, &fixture, mouse);
    let sql = sql_state(&fixture);
    let original_git = git_commands(&fixture);
    choose(&mut session, mouse, "[v Native diff]", b"v");
    session.wait_for_command_count(&fixture, 58);
    nready(&mut session, "new-no-newline");
    no_intermediate_images(&session, mouse);
    focus(&mut session, mouse, "old-no-newline");
    act(&mut session, mouse, NATIVE, "[h Hunk]", b"h");
    nready(&mut session, "Selected 2");
    let selected = actual_selected(&session);
    assert!(selected.contains("old-no-newline"));
    let host = session.actual_terminal_witness_v4().unwrap();
    assert!(host.is_raw());
    chosen(&mut session, mouse, false);
    external(&mut session, "EXTERNAL PAGER READY");
    let record = tools.wait_phase(&mut session, "delta", "body", 1).remove(0);
    assert_eq!(record["input"], hex(&history.patch));
    let whole = steps
        .iter()
        .find(|s| s.args.first().is_some_and(|a| a == "pr"))
        .unwrap();
    assert_ne!(record["input"], hex(&whole.stdout));
    assert!(whole
        .stdout
        .windows(b"z-other.rs".len())
        .any(|w| w == b"z-other.rs"));
    let entered = tools.wait_phase(&mut session, "less", "pty", 1).remove(0);
    let owned = identity(&session, &entered);
    let directory = PathBuf::from(entered["cwd"].as_str().unwrap());
    assert_eq!(session.actual_terminal_witness_v4().unwrap(), host);
    exact_reads(&session, &fixture, &steps);
    assert_eq!(git_commands(&fixture), original_git);
    assert_eq!(sql_state(&fixture), sql);
    input(&mut session, mouse, PANE, "[Esc Return]", b"\x1b");
    nready(&mut session, "Selected 2");
    assert_eq!(actual_selected(&session), selected);
    cleanup(&mut session, &owned, &directory);
    chosen(&mut session, mouse, true);
    external(&mut session, "EXTERNAL PAGER READY");
    let record = tools.wait_phase(&mut session, "difft", "body", 1).remove(0);
    assert_eq!(record["old"], hex(&history.old_bytes));
    assert_eq!(record["new"], hex(&history.new_bytes));
    assert_ne!(history.m, history.b);
    assert_ne!(history.m, history.h);
    let entered = tools.wait_phase(&mut session, "less", "pty", 2).remove(1);
    let owned = identity(&session, &entered);
    let directory = PathBuf::from(entered["cwd"].as_str().unwrap());
    assert!(!Path::new(record["cwd"].as_str().unwrap()).exists());
    input(&mut session, mouse, PANE, "[^C Cancel]", b"\x03");
    nready(&mut session, "Selected 2");
    assert_eq!(actual_selected(&session), selected);
    cleanup(&mut session, &owned, &directory);
    assert_eq!(session.actual_terminal_witness_v4().unwrap(), host);
    assert_eq!(readonly_git(&fixture), git);
    assert_eq!(git_commands(&fixture), original_git);
    assert_eq!(sql_state(&fixture), sql);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    exact_reads(&session, &fixture, &steps);
    assert!(!session
        .transcript
        .windows(b"PRIVATE-EXTERNAL-TITLE".len())
        .any(|w| w == b"PRIVATE-EXTERNAL-TITLE"));
    assert!(!session
        .transcript
        .windows(b"PRIVATE-CLIPBOARD".len())
        .any(|w| w == b"PRIVATE-CLIPBOARD"));
    assert!(!session
        .transcript
        .windows(b"\x1b[?1049l".len())
        .any(|w| w == b"\x1b[?1049l"));
    act(&mut session, mouse, NATIVE, "[Esc Close]", b"\x1b");
    overlay_ready(&mut session, "Immutable file images", &["[Esc Close]"]);
    act(
        &mut session,
        mouse,
        "Immutable file images",
        "[Esc Close]",
        b"\x1b",
    );
    ready(&mut session, &["[f Files]", "[v Native diff]"]);
    session.quit_and_assert_restored_terminal(mouse);
}
