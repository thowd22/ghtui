//! SOURCE ONLY / UNRUN. Actual binary/real Git/strict gh/SQLite/TTY; these
//! local-step families do NOT substitute for the required remote push/PR flow.
use super::*;
#[path = "scoped_actions_workflow_edit_remote_v9.rs"]
mod remote_v9;
use crate::support::completed_navigation_controls_v1::{owned_focus_v1, FocusAttributeV1};
use std::os::unix::{ffi::OsStrExt, fs::PermissionsExt};
const FORM: &str = "Edit workflow locally • Preview each exact step";
const OLD:&str="name: ACTUAL_PRIVATE_WORKFLOW\non: [push]\njobs:\n  actual_job:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo NEVER_EXECUTED_WORKFLOW\n";
const EDIT:&str="name: ACTUAL_PRIVATE_WORKFLOW\non: [push]\njobs:\n  actual_job:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo PRIVATE_NATIVE_W9_EXACT_EDIT\n";
const LABELS: [&str; 9] = [
    "Local choice: isolated worktree or explicit current directory",
    "Exact edited workflow YAML (private; not a remote write)",
    "Exact NEW branch (no replacement)",
    "Exact commit message (hooks/signing remain enabled)",
    "Proposed PR title (not yet published)",
    "Proposed PR body (not yet published)",
    "Catalogue head repository (Preview rechecks permission/network)",
    "Exact configured push remote name (no inferred origin)",
    "Exact base branch name (fresh actual ref checked at Preview)",
];
const CONTROLS: [&str; 4] = [
    "[Preview next step]",
    "[Editor help]",
    "[Confirm step]",
    "[Keep draft / Close]",
];
fn workflow() -> Value {
    json!({"id":7,"node_id":"WF_7","name":"ACTUAL_PRIVATE_WORKFLOW","path":".github/workflows/ci.yml","state":"active"})
}
fn wf() -> GhStep {
    get(&format!("repos/{CURRENT}/actions/workflows/7"), workflow())
}
fn definition(c: &str) -> Vec<GhStep> {
    resources_v2::resource(
        vec![
            wf(),
            get(&format!("repos/{CURRENT}/commits/main"), json!({"sha":c})),
            GhStep::read(
                &[
                    "api",
                    &format!("repos/{CURRENT}/contents/.github/workflows/ci.yml?ref={c}"),
                    "--hostname",
                    HOST,
                    "--method",
                    "GET",
                    "--header",
                    "Accept: application/vnd.github.raw+json",
                    "--allow-escape-sequences",
                ],
                OLD,
            ),
            wf(),
        ],
        42,
    )
}
fn phase(confirm: bool) -> Vec<GhStep> {
    let mut out = resources_v2::resource(vec![wf(), wf()], 42);
    assert_eq!(out.len(), 8);
    if confirm {
        out.push(get("user", json!({"id":42,"login":"octocat"})));
    }
    out
}
fn fixture(tail: impl FnOnce(&str) -> Vec<GhStep>) -> (Fixture, Vec<GhStep>, String) {
    use ghtui::{
        git::RawPath,
        search::{
            corpus::LocalSource,
            managed::{CorpusOwnership, CorpusRepository},
        },
    };
    use sha2::{Digest, Sha256};
    let fixture = configured_native(&[]);
    fs::create_dir_all(fixture.repo().join(".github/workflows")).unwrap();
    for path in [
        fixture.repo(),
        fixture.repo().join(".github"),
        fixture.repo().join(".github/workflows"),
    ] {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    fs::write(fixture.repo().join(".github/workflows/ci.yml"), OLD).unwrap();
    fixture.git(&["add", "--", ".github/workflows/ci.yml"]);
    // Preserve the fixture's independently staged canary, not commit it with
    // the workflow merely because it was already in the real user's index.
    fixture.git(&[
        "commit",
        "--only",
        "-m",
        "Actual native workflow C",
        "--",
        ".github/workflows/ci.yml",
    ]);
    let c = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let scope = Scope::github(HOST, Some(42)).unwrap();
    let mapping = CorpusRepository {
        scope: scope.clone(),
        repository: FIRST.into(),
        directory: RawPath(fixture.repo().as_os_str().as_bytes().to_vec()),
        source: LocalSource::Revision(c.clone()),
        ownership: CorpusOwnership::Existing,
        active: true,
        saved_commit: Some(c.clone()),
        disk_bytes: 0,
        disk_complete: false,
        clone_source: None,
    };
    Cache::open(&database(&fixture))
        .unwrap()
        .save_view(
            &scope,
            &format!("corpus-mapping-v1:{:x}", Sha256::digest(FIRST.as_bytes())),
            &serde_json::to_value(mapping).unwrap(),
        )
        .unwrap();
    let mut steps = actions_protocol();
    steps.extend(resources_v2::resource(
        vec![paged(
            &format!("repos/{CURRENT}/actions/workflows?per_page=100"),
            json!({"total_count":1,"workflows":[workflow()]}),
        )],
        42,
    ));
    steps.extend(definition(&c));
    steps.extend(definition(&c));
    assert_eq!(steps.len(), 41);
    steps.extend(tail(&c));
    fixture.scripted_gh(&steps);
    (fixture, steps, c)
}
fn body(s: &Session) -> Option<String> {
    s.completed_cells_screen()
        .and_then(|frame| crate::support::overlay_body(frame, FORM))
}
fn cells(s: &mut Session, after: usize, words: &[&str]) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && body(s).is_some_and(|body| {
                CONTROLS.iter().all(|label| body.contains(label))
                    && !body.contains("[Esc Cancel]")
                    && words.iter().all(|word| body.contains(word))
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "NEW actual complete owned W9 Form {words:?}\n{}",
            s.screen()
        );
    }
}
fn labels(body: &str) -> Vec<&'static str> {
    let mut labels = if body.contains("[F4 View]") {
        if body.contains("[Preview next step]") {
            vec![
                "[Preview next step]",
                "[Confirm step]",
                "[F4 View]",
                "[Keep draft / Close]",
            ]
        } else {
            vec!["[F4 View]", "[F6 ACK local]", "[Keep draft / Close]"]
        }
    } else {
        CONTROLS.to_vec()
    };
    if body.contains("[F5 Fields↑]") {
        labels.extend(["[F5 Fields↑]", "[F6 Fields↓]"]);
    }
    if body.contains("[F7 Error↑]") {
        labels.extend(["[F7 Error↑]", "[F8 Error↓]"]);
    }
    labels
}
fn focused_field(frame: &vt100::Screen) -> Option<usize> {
    crate::support::overlay_body(frame, FORM)?;
    let mut result = None;
    for (index, label) in LABELS.iter().enumerate() {
        let Some((x, y)) = crate::support::form_field_value_position(frame, FORM, label) else {
            continue;
        };
        let right = (x..frame.size().1)
            .find(|col| {
                frame
                    .cell(y, *col)
                    .is_some_and(|cell| cell.contents() == "│")
            })?
            .checked_sub(1)?;
        if x < right
            && (x..right).all(|col| frame.cell(y, col).is_some_and(|cell| cell.underline()))
            && result.replace(index).is_some()
        {
            return None;
        }
    }
    result
}
fn field(s: &mut Session, mouse: bool, index: usize) {
    let until = Instant::now() + Duration::from_secs(8);
    for _ in 0..16 {
        let frame = s
            .completed_cells_screen()
            .expect("Actual completed W9 field frame");
        let body = crate::support::overlay_body(frame, FORM).expect("SAME topmost W9 Form");
        let current = focused_field(frame);
        if current == Some(index) {
            return;
        }
        let controls = labels(&body);
        let after = s.completed_cells_end();
        if mouse && crate::support::form_field_value_position(frame, FORM, LABELS[index]).is_some()
        {
            let (x, y) =
                crate::support::form_field_value_position(frame, FORM, LABELS[index]).unwrap();
            s.click(x, y);
        } else {
            let forward = current.is_none_or(|value| value < index);
            let label = if forward {
                "[F6 Fields↓]"
            } else {
                "[F5 Fields↑]"
            };
            let owned = owned_focus_v1(
                frame,
                FORM,
                label,
                FocusAttributeV1::Inverse,
                &controls,
                &[],
            )
            .expect("Actual whole field paging action in SAME owned Form");
            if mouse {
                s.click(owned.target.0, owned.target.1);
            } else {
                s.send_key(if forward { b"\x1b[17~" } else { b"\x1b[15~" });
            }
        }
        loop {
            s.poll_output(Duration::from_millis(1));
            if s.completed_cells_end() > after
                && s.completed_cells_screen().is_some_and(|frame| {
                    crate::support::overlay_body(frame, FORM).is_some()
                        && focused_field(frame).is_some_and(|next| Some(next) != current)
                })
            {
                break;
            }
            assert!(Instant::now()<until,"ONE W9 field input requires NEW actual changed underlined logical field; not a cached cursor coordinate\n{}",s.screen());
        }
    }
    panic!("Actual W9 field {} not reached", LABELS[index]);
}
fn action(s: &mut Session, mouse: bool, label: &str) {
    action_with_activation(s, mouse, label, b"\r")
}
fn action_with_activation(s: &mut Session, mouse: bool, label: &str, activation: &[u8]) {
    if mouse {
        let (x, y) = s.wait_for_owned_completed_action(FORM, label, &[]);
        s.click(x, y);
        return;
    }
    let until = Instant::now() + Duration::from_secs(8);
    for _ in 0..24 {
        let frame = s.completed_cells_screen().expect("Actual completed Form");
        let body = crate::support::overlay_body(frame, FORM).expect("Current owned W9 Form");
        let controls = labels(&body);
        let focus = owned_focus_v1(
            frame,
            FORM,
            label,
            FocusAttributeV1::Inverse,
            &controls,
            &[],
        )
        .expect("Whole visible W9 target before keyboard input");
        if focus.target_focused {
            s.send_key(activation);
            return;
        }
        let previous_field = focused_field(frame);
        let after = s.completed_cells_end();
        s.send_key(b"\t");
        loop {
            s.poll_output(Duration::from_millis(1));
            if s.completed_cells_end() > after
                && s.completed_cells_screen().is_some_and(|frame| {
                    owned_focus_v1(
                        frame,
                        FORM,
                        label,
                        FocusAttributeV1::Inverse,
                        &controls,
                        &[],
                    )
                    .is_some_and(|next| {
                        next.acknowledges_change_from(&focus)
                            || (next.target == focus.target
                                && focused_field(frame)
                                    .is_some_and(|field| Some(field) != previous_field))
                    })
                })
            {
                break;
            }
            assert!(
                Instant::now() < until,
                "ONE Tab requires NEW same-owned actual footer/underlined field change\n{}",
                s.screen()
            );
        }
    }
    panic!("Inverse W9 footer target not reached: {label}");
}
fn replace(s: &mut Session, mouse: bool, index: usize, text: &str) {
    field(s, mouse, index);
    let after = s.completed_cells_end();
    s.send_key(b"\x01");
    s.paste(text);
    cells(s, after, &[LABELS[index]]);
    assert_eq!(
        focused_field(s.completed_cells_screen().unwrap()),
        Some(index)
    );
}
fn initial(s: &mut Session, f: &Fixture, mouse: bool, c: &str) {
    s.resize(120, 40);
    s.wait_for_main_frame();
    let after = s.completed_cursorless_end();
    resources_v2::action(s, mouse, "Workflows", b"W");
    s.wait_for_command_count(f, 21);
    s.wait_for_new_cursorless_condition(
        after,
        |frame| {
            let text = frame.contents();
            text.contains("ACTUAL_PRIVATE_WORKFLOW")
                && text.contains(".github/workflows/ci.yml")
                && !text.contains("Requested ref main")
        },
        "Actual completed workflow inventory",
        Duration::from_secs(8),
    );
    let after = s.completed_cursorless_end();
    selected_row(s, mouse, "ACTUAL_PRIVATE_WORKFLOW");
    s.wait_for_command_count(f, 31);
    s.wait_for_new_cursorless_condition(
        after,
        |frame| {
            let text = frame.contents();
            text.contains("Requested ref main")
                && text.contains(c)
                && text.contains("Workflow definition")
                && !text.contains("[Esc Cancel]")
                && !text.contains("Action unavailable")
                && crate::support::main_footer_visible(frame)
                && crate::support::locate_control(frame, "[E Edit YAML locally]").is_some()
        },
        "Actual owned nonbusy Definition/full C with its Edit key-bar token",
        Duration::from_secs(8),
    );
    let after = s.completed_cells_end();
    activate_definition_edit(s, f, mouse, c);
    s.wait_for_command_count(f, 41);
    cells(s, after, &[LABELS[0], "isolated", "[Space Next]"]);
}
fn activate_definition_edit(s: &mut Session, f: &Fixture, mouse: bool, c: &str) {
    const TITLE: &str = "Actions • focused view ─";
    const EDIT: &str = "[E Edit YAML locally]";
    let frame = s
        .completed_cursorless_screen()
        .expect("Actual completed Definition page");
    let body = crate::support::overlay_body(frame, TITLE)
        .expect("SAME owned actual Actions Definition body");
    assert!(
        body.contains("Requested ref main") && body.contains(c) && !body.contains("[Esc Cancel]"),
        "The Definition keeps the SAME full original Definition/C and nonbusy state"
    );
    assert!(!frame.contents().contains(FORM));
    assert_eq!(
        commands(s, f).len(),
        31,
        "Showing the Definition controls is local and cannot read or mutate GH"
    );
    // Edit is a key-bar token (or, when it does not fit, a palette entry); its key is E.
    s.activate_completed_actions_control(mouse, EDIT, b"E");
}
fn unrelated(f: &Fixture) -> Vec<(String, Vec<Vec<rusqlite::types::Value>>)> {
    let mut rows = sql(f);
    rows.retain(|(name, _)| name != "drafts");
    let db = publication_v3::raw(f);
    let mut query=db.prepare("SELECT host,login,resource,body FROM drafts WHERE resource NOT LIKE 'scoped-actions-v9/%' ORDER BY host,login,resource").unwrap();
    let drafts = query
        .query_map([], |r| {
            (0..4)
                .map(|i| r.get(i))
                .collect::<rusqlite::Result<Vec<rusqlite::types::Value>>>()
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows.push(("unrelated_drafts".into(), drafts));
    rows
}
fn exact(s: &Session, f: &Fixture, steps: &[GhStep]) {
    let actual = commands(s, f);
    assert_eq!(actual.len(), steps.len());
    for (row, expected) in actual.iter().zip(steps) {
        assert_eq!(row["args"], json!(expected.args));
        assert_eq!(row["matched"], true);
        assert_eq!(row["stdin_hex"], "");
    }
    assert!(
        !actual.iter().any(
            |row| row["args"].as_array().unwrap().iter().any(|arg| matches!(
                arg.as_str(),
                Some("POST" | "PATCH" | "PUT" | "DELETE" | "graphql")
            ))
        ),
        "Local edit/Save/Stage/Commit never becomes a remote mutation"
    );
}
fn isolated_root(f: &Fixture) -> std::path::PathBuf {
    let list = f.git(&["worktree", "list", "--porcelain", "-z"]);
    let mut root = None;
    for field in list.split('\0') {
        if let Some(path) = field.strip_prefix("worktree ") {
            root = Some(std::path::PathBuf::from(path));
        }
        if field == "branch refs/heads/workflow/edit" {
            return root.expect("Actual paid worktree branch must have its genuine root");
        }
    }
    panic!("Expected exactly observed new isolated worktree/branch, never default path inference");
}
fn close(s: &mut Session, mouse: bool) {
    let after = s.completed_cells_end();
    action(s, mouse, "[Keep draft / Close]");
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|frame| {
                crate::support::overlay_body(frame, FORM).is_none()
                    && crate::support::main_footer_visible(frame)
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "Actual W9 Close releases usable terminal controls\n{}",
            s.screen()
        );
    }
}
fn local_step(
    s: &mut Session,
    f: &Fixture,
    mouse: bool,
    count: &mut usize,
    preview_words: &[&str],
    receipt_words: &[&str],
) {
    let after = s.completed_cells_end();
    action(s, mouse, "[Preview next step]");
    *count += 8;
    s.wait_for_command_count(f, *count);
    preview_facts_v248(s, f, mouse, after, preview_words);
    let after = s.completed_cells_end();
    action(s, mouse, "[Confirm step]");
    *count += 9;
    s.wait_for_command_count(f, *count);
    cells(s, after, receipt_words);
}
fn preview_facts_v248(s: &mut Session, f: &Fixture, mouse: bool, after: usize, words: &[&str]) {
    cells(s, after, &[]);
    let before = (sql(f), git_state(f), commands(s, f));
    let until = Instant::now() + Duration::from_secs(8);
    // Long genuine private-root paths can wrap above the complete replacement
    // YAML. Every fact must be visible in this SAME real Preview, not hidden
    // source text; only actual viewport input may expose a later complete row.
    for word in words {
        loop {
            let current = body(s).expect("SAME completed owned local Preview");
            assert!(CONTROLS.iter().all(|label| current.contains(label)));
            assert!(!current.contains("[Esc Cancel]"));
            if current.contains(word) {
                break;
            }
            let after = s.completed_cells_end();
            let (rows, cols) = s.completed_cells_screen().unwrap().size();
            if mouse {
                s.wheel(cols / 2, rows / 2, true);
            } else {
                s.send_key(b"\x1b[6~");
            }
            loop {
                s.poll_output(Duration::from_millis(1));
                if s.completed_cells_end() > after && body(s).is_some_and(|next| next != current) {
                    break;
                }
                assert!(
                    Instant::now() < until,
                    "ONE Preview scroll requires NEW changed SAME owned body for full {word:?}\n{}",
                    s.screen()
                );
            }
            assert!(
                Instant::now() < until,
                "Full exact Preview fact must be visible: {word:?}\n{}",
                s.screen()
            );
        }
    }
    assert_eq!((sql(f), git_state(f), commands(s, f)), before);
}
fn vertical(mouse: bool) {
    let (f, steps, c) = fixture(|_| {
        (0..4)
            .flat_map(|_| [phase(false), phase(true)].concat())
            .collect()
    });
    let git = git_state(&f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let mut s = start(&f, mouse);
    initial(&mut s, &f, mouse, &c);
    let sql = unrelated(&f);
    replace(&mut s, mouse, 1, EDIT);
    let mut count = 41;
    local_step(
        &mut s,
        &f,
        mouse,
        &mut count,
        &["NEW branch", "workflow/edit", &c],
        &["Actual isolated worktree/NEW branch observed"],
    );
    let root = isolated_root(&f);
    assert_ne!(root, f.repo());
    assert_eq!(
        fs::read(root.join(".github/workflows/ci.yml")).unwrap(),
        OLD.as_bytes()
    );
    local_step(
        &mut s,
        &f,
        mouse,
        &mut count,
        &[
            "SAVE EXACT WORKFLOW FILE",
            "Complete current WORKING bytes",
            "PRIVATE_NATIVE_W9_EXACT_EDIT",
        ],
        &["Actual exact file Save observed"],
    );
    assert_eq!(
        fs::read(root.join(".github/workflows/ci.yml")).unwrap(),
        EDIT.as_bytes()
    );
    assert!(f
        .git(&[
            "-C",
            root.to_str().unwrap(),
            "diff",
            "--cached",
            "--",
            ".github/workflows/ci.yml"
        ])
        .is_empty());
    local_step(
        &mut s,
        &f,
        mouse,
        &mut count,
        &["Stage selected file", ".github/workflows/ci.yml"],
        &["Actual exact path staged"],
    );
    assert!(f
        .git(&[
            "-C",
            root.to_str().unwrap(),
            "diff",
            "--cached",
            "--",
            ".github/workflows/ci.yml"
        ])
        .contains("+      - run: echo PRIVATE_NATIVE_W9_EXACT_EDIT"));
    local_step(
        &mut s,
        &f,
        mouse,
        &mut count,
        &[
            "Commit staged changes",
            "Complete actual staged canonical patch",
        ],
        &["Actual reviewed C..D commit observed"],
    );
    assert_eq!(count, 109);
    let d = f.git(&["-C", root.to_str().unwrap(), "rev-parse", "HEAD"]);
    assert_ne!(d.trim(), c);
    assert_eq!(
        f.git(&[
            "-C",
            root.to_str().unwrap(),
            "show",
            "HEAD:.github/workflows/ci.yml"
        ]),
        EDIT
    );
    assert_eq!(
        f.git(&["-C", root.to_str().unwrap(), "rev-parse", "HEAD^"])
            .trim(),
        c
    );
    assert_eq!(
        f.git(&["-C", root.to_str().unwrap(), "status", "--porcelain=v1"]),
        ""
    );
    let after = git_state(&f);
    assert_eq!(after.head, git.head);
    assert_eq!(after.index, git.index);
    assert_eq!(after.status, git.status);
    assert_eq!(after.staged_diff, git.staged_diff);
    assert_eq!(after.working_diff, git.working_diff);
    assert_eq!(after.dirty, git.dirty);
    assert_eq!(after.staged, git.staged);
    assert_eq!(after.untracked, git.untracked);
    assert_eq!(
        fs::read(f.repo().join(".github/workflows/ci.yml")).unwrap(),
        OLD.as_bytes()
    );
    assert_eq!(
        fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_eq!(unrelated(&f), sql);
    assert_workflow_private_intents_v276(&f);
    s.capture_completed_terminal_v1("task54-w9-observed-local-commit");
    close(&mut s, mouse);
    s.quit_and_assert_restored_terminal(mouse);
    exact(&s, &f, &steps);
    assert_eq!(unrelated(&f), sql);
    assert_eq!(
        fs::read(root.join(".github/workflows/ci.yml")).unwrap(),
        EDIT.as_bytes()
    );
}
fn assert_workflow_private_intents_v276(f: &Fixture) {
    use sha2::{Digest, Sha256};
    let cache = Cache::open(&database(f)).unwrap();
    let original = Scope::github(HOST, Some(42)).unwrap();
    let c = f.git(&["rev-parse", "HEAD"]).trim().to_owned();
    assert_eq!(
        f.git(&["show", "HEAD:.github/workflows/ci.yml"]),
        OLD,
        "Original root remains at actual fixture C, never isolated D"
    );
    let mapping_key = format!("corpus-mapping-v1:{:x}", Sha256::digest(FIRST.as_bytes()));
    // Independent literal expected model of the ONE mapping installed by
    // fixture() BEFORE launch; never filter arbitrary saved views/mappings.
    let mapping = json!({
        "scope":{"host":HOST,"login":"user-id:42"},
        "repository":FIRST,
        "directory":f.repo().as_os_str().as_bytes(),
        "source":{"Revision":c},
        "ownership":"Existing",
        "active":true,
        "saved_commit":c,
        "disk_bytes":0,
        "disk_complete":false,
        "clone_source":null
    });
    assert_eq!(
        cache.saved_views(&original).unwrap(),
        vec![
            (INTENT_CANARY.into(), original_intent()),
            (mapping_key, mapping)
        ]
    );
    assert_eq!(
        cache.draft(&original, "catalogue-draft").unwrap(),
        Some("Original42 draft".into())
    );
    let other = Scope::github(HOST, Some(43)).unwrap();
    assert!(cache.saved_views(&other).unwrap().is_empty());
    assert!(cache.draft(&other, "catalogue-draft").unwrap().is_none());
}
#[test]
fn keyboard_task54_workflow_edit_v9_actual_isolated_save_stage_commit_preserves_original() {
    vertical(false)
}
#[test]
fn mouse_task54_workflow_edit_v9_actual_isolated_save_stage_commit_preserves_original() {
    vertical(true)
}

fn recover(mouse: bool) {
    let (f, steps, c) = fixture(definition);
    let git = git_state(&f);
    let mut s = start(&f, mouse);
    initial(&mut s, &f, mouse, &c);
    let sql = unrelated(&f);
    let yaml = format!("# {}\n{}", "PRIVATE_RECOVERABLE_YAML".repeat(7_000), EDIT);
    assert!(yaml.len() > 128 * 1024 && yaml.len() < 256 * 1024);
    replace(&mut s, mouse, 1, &yaml);
    close(&mut s, mouse);
    let after = s.completed_cells_end();
    resources_v2::action(&mut s, mouse, "[E Edit YAML locally]", b"E");
    s.wait_for_command_count(&f, 51);
    cells(&mut s, after, &["[Space Next]"]);
    field(&mut s, mouse, 1);
    assert!(body(&s).unwrap().contains("PRIVATE_NATIVE_W9_EXACT_EDIT"),"The genuinely reopened native editor must show recovered edited YAML, not only a saved SQL row");
    let cache = Cache::open(&database(&f)).unwrap();
    let key = "scoped-actions-v9/repository/10000/workflow/7/local-edit";
    let saved = cache
        .draft(&Scope::github(HOST, Some(42)).unwrap(), key)
        .unwrap()
        .expect("Actual accepted save must settle before genuine reopened private draft read");
    assert!(saved.len() > 128 * 1024 && saved.contains(&yaml));
    assert_eq!(git_state(&f), git);
    assert_eq!(unrelated(&f), sql);
    close(&mut s, mouse);
    s.quit_and_assert_restored_terminal(mouse);
    exact(&s, &f, &steps);
    assert_eq!(git_state(&f), git);
}
#[test]
fn keyboard_task54_workflow_edit_v9_pure_close_reopens_actual_large_ordered_draft() {
    recover(false)
}
#[test]
fn mouse_task54_workflow_edit_v9_pure_close_reopens_actual_large_ordered_draft() {
    recover(true)
}

fn exact_saved_edit_v243(f: &Fixture) -> String {
    let mut expected = "ghtui-workflow-edit-draft-v9\n".to_owned();
    for row in [
        "isolated",
        EDIT,
        "workflow/edit",
        "Update workflow",
        "Update workflow",
        "",
    ] {
        expected.push_str(&format!("{}:{row}", row.len()));
    }
    let db = publication_v3::raw(f);
    let actual: String = db.query_row(
        "SELECT body FROM drafts WHERE host=?1 AND login='user-id:42' AND resource='scoped-actions-v9/repository/10000/workflow/7/local-edit'",
        [HOST], |row| row.get(0),
    ).expect("Actual accepted draft is already durably settled before viewport-only navigation");
    assert_eq!(
        actual, expected,
        "All six exact native field bytes, including YAML's trailing LF"
    );
    actual
}

fn show_exact_yaml_tail_v243(s: &mut Session, f: &Fixture) {
    const CANARY: &str = "PRIVATE_NATIVE_W9_EXACT_EDIT";
    assert!(EDIT.ends_with('\n'));
    let saved = exact_saved_edit_v243(f);
    let before = (sql(f), git_state(f), commands(s, f));
    assert_eq!(focused_field(s.completed_cells_screen().unwrap()), Some(1));
    if !body(s).unwrap().contains(CANARY) {
        let after = s.completed_cells_end();
        // Actual editor document-end navigation, never cached cursor cells.
        s.send_key(b"\x1b[1;5F");
        cells(s, after, &[LABELS[1]]);
        assert_eq!(focused_field(s.completed_cells_screen().unwrap()), Some(1));
        if !body(s).unwrap().contains(CANARY) {
            let frame = s.completed_cells_screen().unwrap();
            let (x, y) = crate::support::form_field_value_position(frame, FORM, LABELS[1]).unwrap();
            let blank: String = (x..frame.size().1)
                .map_while(|column| frame.cell(y, column))
                .take_while(|cell| cell.contents() != "│")
                .map(|cell| cell.contents())
                .collect();
            assert!(
                blank.trim().is_empty(),
                "The exact saved trailing-LF final row is actually blank"
            );
            let after = s.completed_cells_end();
            s.send_key(b"\x1b[A");
            cells(s, after, &[LABELS[1], CANARY]);
            assert_eq!(focused_field(s.completed_cells_screen().unwrap()), Some(1));
        }
    }
    assert!(body(s).unwrap().contains(CANARY));
    assert_eq!(exact_saved_edit_v243(f), saved);
    assert_eq!((sql(f), git_state(f), commands(s, f)), before);
}

fn narrow(mouse: bool) {
    let (f, steps, c) = fixture(|_| vec![]);
    let git = git_state(&f);
    let mut s = start(&f, mouse);
    initial(&mut s, &f, mouse, &c);
    let sql = unrelated(&f);
    replace(&mut s, mouse, 1, EDIT);
    let after = s.completed_cells_end();
    action(&mut s, mouse, "[Editor help]");
    cells(&mut s, after, &["no process/file/write was started"]);
    let after = s.completed_cells_end();
    s.resize(80, 20);
    cells(&mut s, after, &["[F7 Error↑]", "[F8 Error↓]"]);
    field(&mut s, mouse, 1);
    show_exact_yaml_tail_v243(&mut s, &f);
    assert!(body(&s).unwrap().contains("PRIVATE_NATIVE_W9_EXACT_EDIT"));
    let after = s.completed_cells_end();
    s.resize_owned_single_action_modal_v2(
        40,
        10,
        "Original Actions • resize required",
        "[Esc Close]",
    );
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|frame| {
                frame.size() == (10, 40)
                    && crate::support::overlay_body(frame, "Original Actions • resize required")
                        .is_some_and(|body| {
                            body.contains("Resize to 80×20")
                                && body.contains("Actions disabled until resized")
                                && body.contains("[Esc Close]")
                                && !body.contains("[Confirm step]")
                        })
            })
        {
            break;
        }
        assert!(
            Instant::now() < until,
            "Actual tiny W9 resize-only, no hidden confirmation\n{}",
            s.screen()
        );
    }
    assert_eq!(commands(&s, &f).len(), 41);
    assert_eq!(git_state(&f), git);
    assert_eq!(unrelated(&f), sql);
    let after = s.completed_cells_end();
    s.resize(80, 24);
    cells(&mut s, after, &["[F7 Error↑]", "[F8 Error↓]"]);
    field(&mut s, mouse, 1);
    show_exact_yaml_tail_v243(&mut s, &f);
    assert!(body(&s).unwrap().contains("PRIVATE_NATIVE_W9_EXACT_EDIT"));
    close(&mut s, mouse);
    s.quit_and_assert_restored_terminal(mouse);
    exact(&s, &f, &steps);
    assert_eq!(git_state(&f), git);
    assert_eq!(unrelated(&f), sql);
}
#[test]
fn keyboard_task54_workflow_edit_v9_narrow_error_editor_refusal_retains_exact_private_input() {
    narrow(false)
}
#[test]
fn mouse_task54_workflow_edit_v9_narrow_error_editor_refusal_retains_exact_private_input() {
    narrow(true)
}

fn failed_save(mouse: bool) {
    let (f, steps, c) = fixture(|_| vec![]);
    let git = git_state(&f);
    let mut s = start(&f, mouse);
    initial(&mut s, &f, mouse, &c);
    let db = rusqlite::Connection::open_with_flags(
        database(&f),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    db.execute_batch("CREATE TRIGGER actual_private_workflow_save_failure BEFORE INSERT ON drafts WHEN NEW.resource LIKE 'scoped-actions-v9/%' BEGIN SELECT RAISE(ABORT,'PRIVATE_TRIGGER_DIAGNOSTIC_YAML_MESSAGE'); END;").unwrap();
    let before = unrelated(&f);
    replace(&mut s, mouse, 1, EDIT);
    replace(&mut s, mouse, 3, "PRIVATE_COMMIT_MESSAGE_NOT_DIAGNOSTIC");
    let after = s.completed_cells_end();
    field(&mut s, mouse, 2);
    // A genuine field move yields a NEW completed frame; wait for the actual
    // ACK's fixed owned error, not a manufactured failure or a sleep.
    cells(&mut s, after, &["Local workflow draft save failed"]);
    let after = s.completed_cells_end();
    s.resize(80, 20);
    cells(
        &mut s,
        after,
        &[
            "Local workflow draft save failed",
            "[F7 Error↑]",
            "[F8 Error↓]",
        ],
    );
    field(&mut s, mouse, 2);
    let visible = body(&s).unwrap();
    assert!(!visible.contains("PRIVATE_TRIGGER_DIAGNOSTIC")&&!visible.contains("PRIVATE_COMMIT_MESSAGE_NOT_DIAGNOSTIC"),"No raw SQLite diagnostic or hidden commit message is republished into the W9 error viewport");
    assert!(
        !visible.contains("PRIVATE_NATIVE_W9_EXACT_EDIT"),
        "The off-page YAML is not copied into the error body"
    );
    let cache = Cache::open(&database(&f)).unwrap();
    let scope = Scope::github(HOST, Some(42)).unwrap();
    let key = "scoped-actions-v9/repository/10000/workflow/7/local-edit";
    assert!(
        cache.draft(&scope, key).unwrap().is_none(),
        "Actual SQLite trigger refusal, not accepted==durable"
    );
    close(&mut s, mouse);
    let after = s.completed_cells_end();
    resources_v2::action(&mut s, mouse, "[E Edit YAML locally]", b"E");
    cells(
        &mut s,
        after,
        &["Actual accepted workflow draft save failed"],
    );
    assert_eq!(commands(&s,&f).len(),41,"Failed accepted Save must restore exact old private text BEFORE ANY fresh Core/GH; no replay or new purpose");
    assert!(cache.draft(&scope, key).unwrap().is_none());
    assert_eq!(git_state(&f), git);
    assert_eq!(unrelated(&f), before);
    db.execute_batch("DROP TRIGGER actual_private_workflow_save_failure;")
        .unwrap();
    // A NEW EXPLICIT input is the only retry. Removing the trigger alone did
    // not replay the failed body or enable a Preview/Confirm automatically.
    replace(&mut s, mouse, 3, "ACTUAL_EXPLICIT_RECOVERY_MESSAGE");
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if cache.draft(&scope, key).unwrap().is_some_and(|raw| {
            raw.contains(EDIT) && raw.contains("ACTUAL_EXPLICIT_RECOVERY_MESSAGE")
        }) {
            break;
        }
        assert!(
            Instant::now() < until,
            "Actual explicit new edit must settle through SAME DraftWriter\n{}",
            s.screen()
        );
    }
    assert_eq!(commands(&s, &f).len(), 41);
    assert_eq!(git_state(&f), git);
    assert_eq!(unrelated(&f), before);
    close(&mut s, mouse);
    s.quit_and_assert_restored_terminal(mouse);
    exact(&s, &f, &steps);
    assert_eq!(git_state(&f), git);
    assert_eq!(unrelated(&f), before);
}
#[test]
fn keyboard_task54_workflow_edit_v9_actual_failed_save_close_restores_without_private_io_or_replay()
{
    failed_save(false)
}
#[test]
fn mouse_task54_workflow_edit_v9_actual_failed_save_close_restores_without_private_io_or_replay() {
    failed_save(true)
}
