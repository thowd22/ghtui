mod support;

#[test]
fn test_framework_control_block_resize_requires_complete_ordered_bottom_controls_and_current_geometry(
) {
    use support::{
        owned_control_block_ready_v3 as ready, owned_resize_controls_ready_v1, ResizeToolbarV1,
    };
    const TITLE: &str = "Owned wrapped controls";
    const ACTIONS: [&str; 3] = ["[A First action]", "[B Second action]", "[C Last action]"];
    fn painted(rows: u16, cols: u16) -> vt100::Parser {
        let mut parser = vt100::Parser::new(rows, cols, 0);
        parser.process(format!("\x1b[1;1H┌ {TITLE}\x1b[1;{cols}H┐").as_bytes());
        let first = if cols == 40 { rows - 2 } else { rows - 1 };
        for row in first..rows {
            parser.process(format!("\x1b[{row};1H│\x1b[{row};{cols}H│").as_bytes());
        }
        // Independent literal layout for these known three controls, not the
        // helper's footprint calculation or a snapshot accepted by default.
        parser.process(format!("\x1b[{first};2H[A First action] [B Second action]").as_bytes());
        if cols == 40 {
            parser.process(format!("\x1b[{};2H[C Last action]", rows - 1).as_bytes());
        } else {
            parser.process(b" [C Last action]");
        }
        parser.process(format!("\x1b[{rows};1H└\x1b[{rows};{cols}H┘").as_bytes());
        parser
    }
    for (rows, cols) in [(18, 40), (24, 80), (40, 120)] {
        let parser = painted(rows, cols);
        assert!(ready(parser.screen(), TITLE, &ACTIONS));
        assert!(owned_resize_controls_ready_v1(
            parser.screen(),
            TITLE,
            &ACTIONS,
            ResizeToolbarV1::ControlBlock
        ));
        if cols == 40 {
            assert!(
                !owned_resize_controls_ready_v1(
                    parser.screen(),
                    TITLE,
                    &ACTIONS,
                    ResizeToolbarV1::Bottom
                ),
                "old single-row semantics are not relaxed"
            );
        }
        assert!(!ready(parser.screen(), "Unrelated owner", &ACTIONS));
        assert!(!ready(
            parser.screen(),
            TITLE,
            &[ACTIONS[1], ACTIONS[0], ACTIONS[2]]
        ));
        assert!(!ready(parser.screen(), TITLE, &[ACTIONS[0], ACTIONS[0]]));
        assert!(!ready(
            parser.screen(),
            TITLE,
            &["[A First", ACTIONS[1], ACTIONS[2]]
        ));
        assert!(!ready(parser.screen(), TITLE, &[ACTIONS[2]]));
        let mut old_geometry = painted(rows, cols);
        old_geometry.set_size(rows + 1, cols);
        assert!(!ready(old_geometry.screen(), TITLE, &ACTIONS));
        let mut body_impostor = painted(rows, cols);
        body_impostor
            .process(format!("\x1b[{};1H\x1b[2K\x1b[4;2H[C Last action]", rows - 1).as_bytes());
        assert!(
            !ready(body_impostor.screen(), TITLE, &ACTIONS),
            "matching body text is not the owned bottom control"
        );
        let mut missing_header = painted(rows, cols);
        missing_header.process(format!("\x1b[1;1H\x1b[2K\x1b[3;2H{TITLE}").as_bytes());
        assert!(
            !ready(missing_header.screen(), TITLE, &ACTIONS),
            "owner title must be on actual first border row"
        );
        let mut missing_corner = painted(rows, cols);
        missing_corner.process(format!("\x1b[{rows};{cols}H ").as_bytes());
        assert!(!ready(missing_corner.screen(), TITLE, &ACTIONS));
        let mut unrelated_body = painted(rows, cols);
        unrelated_body.process(format!("\x1b[4;2H{}", ACTIONS.join(" ")).as_bytes());
        assert!(
            ready(unrelated_body.screen(), TITLE, &ACTIONS),
            "unrelated body copies cannot replace or invalidate exact owned controls"
        );
    }
}

#[test]
fn test_framework_single_action_owned_resize_requires_whole_control_title_location_and_new_geometry(
) {
    use support::{
        complete_single_owned_control_v2 as complete, owned_resize_controls_ready_v1 as ready,
        ResizeToolbarV1,
    };
    let title = "Original review • resize required";
    let action = "[Esc Close]";
    assert!(complete(action));
    for invalid in ["", "[]", "[Esc", "Close]", "[Esc] [Close]", "[Esc\nClose]"] {
        assert!(
            !complete(invalid),
            "not one complete painted action: {invalid:?}"
        );
    }
    for (rows, cols) in [(12, 40), (24, 80)] {
        let mut parser = vt100::Parser::new(rows, cols, 0);
        parser.process(format!("\x1b[1;1H┌ {title}\x1b[{};2H{action}", rows - 1).as_bytes());
        assert!(!ready(
            parser.screen(),
            title,
            &[action],
            ResizeToolbarV1::Bottom
        ));
        parser.process(format!("\x1b[{rows};1H└").as_bytes());
        assert!(!ready(
            parser.screen(),
            title,
            &[action],
            ResizeToolbarV1::Bottom
        ));
        parser.process(format!("\x1b[{rows};{cols}H┘").as_bytes());
        assert!(ready(
            parser.screen(),
            title,
            &[action],
            ResizeToolbarV1::Bottom
        ));
        assert!(!ready(
            parser.screen(),
            "Unrelated modal",
            &[action],
            ResizeToolbarV1::Bottom
        ));
        assert!(!ready(
            parser.screen(),
            title,
            &["[Esc Back]"],
            ResizeToolbarV1::Bottom
        ));
        parser.process(format!("\x1b[{};1H\x1b[2K\x1b[4;2H{action}", rows - 1).as_bytes());
        assert!(!ready(
            parser.screen(),
            title,
            &[action],
            ResizeToolbarV1::Bottom
        ));
        parser.process(format!("\x1b[{};2H{action}", rows - 1).as_bytes());
        assert!(ready(
            parser.screen(),
            title,
            &[action],
            ResizeToolbarV1::Bottom
        ));
        parser.set_size(rows + 1, cols);
        assert!(
            !ready(parser.screen(), title, &[action], ResizeToolbarV1::Bottom),
            "old-size border/control cannot acknowledge the new geometry"
        );
    }
}

#[test]
fn test_framework_owned_resize_toolbar_requires_declared_location_title_and_new_bottom_border() {
    use support::{owned_resize_controls_ready_v1 as ready, ResizeToolbarV1};
    for (rows, cols) in [(24, 80), (40, 120)] {
        let mut parser = vt100::Parser::new(rows, cols, 0);
        let title = "PR review threads • read only";
        let actions = ["[Esc Back]", "[q Quit]"];
        parser.process(format!("\x1b[1;1H┌ {title}\x1b[2;2H[Esc Back] [q Quit]").as_bytes());
        assert!(!ready(
            parser.screen(),
            title,
            &actions,
            ResizeToolbarV1::Header
        ));
        parser.process(format!("\x1b[{rows};1H└\x1b[{rows};{cols}H┘").as_bytes());
        assert!(ready(
            parser.screen(),
            title,
            &actions,
            ResizeToolbarV1::Header
        ));
        assert!(!ready(
            parser.screen(),
            title,
            &actions,
            ResizeToolbarV1::Bottom
        ));
        parser
            .process(format!("\x1b[2;1H\x1b[2K\x1b[{};2H[Esc Back] [q Quit]", rows - 1).as_bytes());
        assert!(ready(
            parser.screen(),
            title,
            &actions,
            ResizeToolbarV1::Bottom
        ));
        assert!(!ready(
            parser.screen(),
            title,
            &actions,
            ResizeToolbarV1::Header
        ));
        parser.process(b"\x1b[1;1H\x1b[2K\x1b[2;2H[Esc Back] [q Quit]");
        assert!(!ready(
            parser.screen(),
            title,
            &actions,
            ResizeToolbarV1::Header
        ));
    }
}

#[test]
fn test_framework_git_maintenance_environment_and_actual_git_contracts() {
    let result = std::process::Command::new("node")
        .args(["--test", "tests/test_env.test.mjs"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.contains("# pass 5"), "{output}");
    assert!(output.contains("# fail 0"), "{output}");
    assert!(output.contains("# skipped 0"), "{output}");
}

#[test]
fn test_framework_form_field_locator_requires_owned_phase_column_and_footer() {
    let mut terminal = vt100::Parser::new(24, 120, 0);
    terminal.process(b"\x1b[2J\x1b[HDecision explanation: old history\x1b[4;13HACTIONS INTENT\x1b[9;1HDecision explanation: underlying history");
    assert!(support::form_field_value_position(
        terminal.screen(),
        "ACTIONS INTENT",
        "Decision explanation"
    )
    .is_none());
    terminal.process(b"\x1b[22;13H[Preview] [Keep draft / Close]");
    assert!(support::form_field_value_position(
        terminal.screen(),
        "ACTIONS INTENT",
        "Decision explanation"
    )
    .is_none());
    terminal.process(b"\x1b[7;13HDecision explanation:");
    assert_eq!(
        support::form_field_value_position(
            terminal.screen(),
            "ACTIONS INTENT",
            "Decision explanation"
        ),
        Some((12, 7))
    );
    terminal.process(b"\x1b[7;13HDecision explanation (optional):");
    assert_eq!(
        support::form_field_value_position(
            terminal.screen(),
            "ACTIONS INTENT",
            "Decision explanation"
        ),
        Some((12, 7)),
        "Stable short labels can omit the actual parenthesized field caption"
    );
    assert!(
        support::form_field_value_position(terminal.screen(), "ACTIONS INTENT", "Decision")
            .is_none(),
        "A prefix of an unrelated complete word is not an actual field"
    );
    assert!(support::form_field_value_position(
        terminal.screen(),
        "PR INTENT",
        "Decision explanation"
    )
    .is_none());
    terminal.process(b"\x1b[22;1H\x1b[2K");
    assert!(support::form_field_value_position(
        terminal.screen(),
        "ACTIONS INTENT",
        "Decision explanation"
    )
    .is_none());
}

#[test]
fn test_framework_pr_menu_selection_ignores_underlying_and_incomplete_markers() {
    for padding in ["", "                      "] {
        let frame = format!(
            "│> Existing selected comment │\n{padding}┌ PR actions • native workflows ┐\n│> E{padding}│ > Add label                    │ background │\n{padding}│ [Esc Close]                   │\n{padding}└───────────────────────────────┘\n"
        );
        assert_eq!(
            support::selected_pr_menu_row(&frame).as_deref(),
            Some("Add label")
        );
        assert_eq!(
            support::selected_pr_menu_row(&frame.replace("│ > Add label", "│ > Remove label"))
                .as_deref(),
            Some("Remove label")
        );
        assert_eq!(
            support::selected_pr_menu_row(
                &frame.replace("│ [Esc Close]", "│ > Remove label │\n│ [Esc Close]")
            ),
            None,
            "A partially repainted pair of selected rows cannot acknowledge movement"
        );
        assert_eq!(
            support::selected_pr_menu_row(&frame.replace("Add label", "Add label [disabled]"))
                .as_deref(),
            Some("Add label [disabled]")
        );
    }
    assert_eq!(support::selected_pr_menu_row("│> Selected comment │"), None);
    assert_eq!(
        support::selected_pr_menu_row("┌ PR actions • native workflows ┐\n│ > Add label"),
        None,
        "Incomplete owned row has no closing border"
    );
}

#[test]
fn test_framework_parallel_journey_records_are_complete_unique_json_lines() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journeys.jsonl");
    std::thread::scope(|scope| {
        for worker in 0..8 {
            let path = &path;
            scope.spawn(move || {
                for journey in 0..32 {
                    let record = serde_json::json!({
                        "test": format!("worker-{worker}/journey-{journey}"),
                        "keyboard": worker % 2 == 0,
                        "mouse": worker % 2 != 0,
                        "exited": true,
                        "panicking": false,
                        "detail": "雪🙂".repeat(2048),
                    });
                    support::append_journey_record(path, &record).unwrap();
                }
            });
        }
    });
    let journal = std::fs::read_to_string(&path).unwrap();
    assert!(journal.ends_with('\n'));
    let mut observed = std::collections::BTreeSet::new();
    for line in journal.lines() {
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        let name = value["test"].as_str().unwrap();
        assert!(observed.insert(name.to_owned()), "Duplicate journey record");
        assert_eq!(value["exited"], true);
        assert_eq!(value["panicking"], false);
        assert_eq!(value["detail"], "雪🙂".repeat(2048));
    }
    assert_eq!(observed.len(), 256);
    assert!(support::append_journey_record(
        &temp.path().join("missing-parent/journeys.jsonl"),
        &serde_json::json!({"test":"cannot-record"}),
    )
    .is_err());
}

#[test]
fn test_framework_restored_body_without_actual_bottom_toolbar_is_not_complete_frame() {
    for (rows, cols) in [(24, 80), (40, 120)] {
        let mut terminal = vt100::Parser::new(rows, cols, 0);
        terminal.process(b"\x1b[2J\x1b[HRepository - focused view");
        terminal.process(format!("\x1b[{};1H[q Quit] [Esc Close]", rows - 1).as_bytes());
        assert!(
            !support::main_footer_visible(terminal.screen()),
            "An old modal Quit is not the restored main toolbar"
        );
        terminal.process(format!("\x1b[{rows};1H[? Help]").as_bytes());
        assert!(
            !support::main_footer_visible(terminal.screen()),
            "Partial footer paint still lacks its actual Quit target"
        );
        terminal.process(format!("\x1b[{rows};48H[q Quit]").as_bytes());
        assert!(support::main_footer_visible(terminal.screen()));
        assert_eq!(
            support::locate_screen(terminal.screen(), "[? Help]"),
            Some((0, rows - 1))
        );
    }
}

#[test]
fn test_framework_overlay_body_keeps_erased_cell_spaces_unicode_and_exact_borders() {
    let mut terminal = vt100::Parser::new(8, 80, 0);
    terminal.process(b"\x1b[2J\x1b[H");
    let width = 48;
    let title = "Source • detail";
    terminal.process(
        format!(
            "\x1b[1;1H┌{title}{}┐",
            "─".repeat(width - 2 - title.chars().count())
        )
        .as_bytes(),
    );
    for row in 2..=4 {
        terminal.process(format!("\x1b[{row};1H│\x1b[{row};{width}H│").as_bytes());
    }
    terminal.process(format!("\x1b[5;1H└{}┘", "─".repeat(width - 2)).as_bytes());
    // Actual cursor gaps, not a string filled with literal spaces.
    terminal.process("\x1b[2;3H[Esc\x1b[2;8HClose]\x1b[3;3H雪🙂\x1b[3;9He\u{301}\x1b[3;12Hword\x1b[3;50HBACKGROUND".as_bytes());
    assert!(terminal.screen().cell(1, 6).unwrap().contents().is_empty());
    let body = support::overlay_body(terminal.screen(), title).unwrap();
    assert!(body.contains("[Esc Close]"));
    assert!(!body.contains("[EscClose]"));
    assert!(body.contains("雪🙂  e\u{301}  word"));
    assert!(!body.contains("BACKGROUND") && !body.contains(title));
    assert_eq!(body.lines().count(), 3);
    assert!(support::overlay_body(terminal.screen(), "missing title").is_none());
    terminal.process(b"\x1b[5;48H ");
    assert!(
        support::overlay_body(terminal.screen(), title).is_none(),
        "An incomplete observed box cannot satisfy readiness"
    );
}

#[test]
fn test_framework_first_form_field_refuses_old_visible_sibling_and_requires_current_value() {
    let mut terminal = vt100::Parser::new(24, 80, 0);
    terminal.process("REPOSITORY INTENT\r\n│ Operation (explicit preview) │\r\n│ map │\r\n│ Clone origin │\r\n│ github │".as_bytes());
    let observed = terminal.screen().contents();
    assert_eq!(
        support::first_form_field_value(&observed, "REPOSITORY INTENT", "Operation"),
        Some("map")
    );
    assert_eq!(
        support::first_form_field_value(&observed, "REPOSITORY INTENT", "Clone origin"),
        None
    );
    terminal.process("\x1b[2J\x1b[HREPOSITORY INTENT\r\n│ Clone origin │".as_bytes());
    assert_eq!(
        support::first_form_field_value(
            &terminal.screen().contents(),
            "REPOSITORY INTENT",
            "Clone origin"
        ),
        None
    );
    terminal.process("\r\n│ local-bare │".as_bytes());
    let observed = terminal.screen().contents();
    assert_eq!(
        support::first_form_field_value(&observed, "REPOSITORY INTENT", "Clone origin"),
        Some("local-bare")
    );
    assert_eq!(
        support::first_form_field_value(&observed, "ACTIONS INTENT", "Clone origin"),
        None
    );
}
use std::process::{Command, Stdio};
use support::{Fixture, GhStep};

#[test]
fn test_framework_unicode_locator_preserves_real_wide_and_combining_cell_coordinates() {
    let mut terminal = vt100::Parser::new(8, 80, 0);
    terminal.process("\u{1b}[3;1H雪e\u{301}🙂 \u{1b}[7m[Confirm action]\u{1b}[0m".as_bytes());
    assert_eq!(
        support::locate_screen(terminal.screen(), "[Confirm action]"),
        Some((6, 2))
    );
    assert!(terminal.screen().cell(2, 6).unwrap().inverse());
    assert_eq!(
        support::locate_screen(terminal.screen(), "雪"),
        Some((0, 2))
    );
    assert_eq!(
        support::locate_screen(terminal.screen(), "e\u{301}"),
        Some((2, 2))
    );
    assert_eq!(
        support::locate_screen(terminal.screen(), "🙂"),
        Some((3, 2))
    );
    assert_eq!(
        support::locate_screen(terminal.screen(), "[Nonexistent action]"),
        None
    );
}

#[test]
fn test_framework_independent_dirty_status_preserves_exact_index_bytes() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"saved bytes\n", "Saved fixture");
    std::fs::write(
        fixture.repo().join("file.txt"),
        b"different dirty working bytes\n",
    )
    .unwrap();
    let index = std::fs::read(fixture.repo().join(".git/index")).unwrap();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    assert!(fixture
        .git(&["status", "--porcelain=v2", "-z"])
        .contains("file.txt"));
    assert_eq!(
        std::fs::read(fixture.repo().join(".git/index")).unwrap(),
        index
    );
    assert_eq!(fixture.git(&["show", ":file.txt"]), "saved bytes\n");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(
        std::fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"different dirty working bytes\n"
    );
}

#[test]
fn dependency_transport_verifies_portable_pins_licenses_and_locked_source_checksums() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let result = Command::new("node")
        .arg(root.join("scripts/audit-dependencies.mjs"))
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let summary: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    let lock: toml::Value =
        toml::from_str(&std::fs::read_to_string(root.join("Cargo.lock")).unwrap()).unwrap();
    let registry_packages = lock["package"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|package| {
            package
                .get("source")
                .and_then(toml::Value::as_str)
                .is_some_and(|source| source.starts_with("registry+"))
        })
        .count();
    assert!(registry_packages > 0);
    assert_eq!(summary["registryPackages"], registry_packages);
    let cargo: toml::Value =
        toml::from_str(&std::fs::read_to_string(root.join("Cargo.toml")).unwrap()).unwrap();
    assert_eq!(
        summary["patches"],
        cargo["patch"]["crates-io"].as_table().unwrap().len()
    );
    assert!(summary["upstreamSources"].as_u64().unwrap() > 0);
    assert!(summary["upstreamFiles"].as_u64().unwrap() > 0);
}

#[test]
fn dependency_transport_rejects_corrupt_files_and_escaping_source_paths() {
    let fixture = Fixture::new();
    std::fs::write(fixture.root.join("source.txt"), b"corrupted source\n").unwrap();
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/audit-dependencies.mjs");
    let result = Command::new("node")
        .args([
            "--input-type=module",
            "-e",
            r#"
import { pathToFileURL } from 'node:url';
const { safePath, verifyFileChecksum } = await import(pathToFileURL(process.env.AUDIT_SCRIPT));
let rejected = 0;
for (const attempt of [
  () => safePath(process.env.CHECK_ROOT, '../outside'),
  () => safePath(process.env.CHECK_ROOT, '/tmp'),
  () => verifyFileChecksum(process.env.CHECK_ROOT, 'source.txt', '0'.repeat(64)),
]) { try { attempt(); } catch { rejected++; } }
if (rejected !== 3) throw new Error(`only ${rejected} corrupt/path-escape requests rejected`);
console.log('3 unsafe source requests rejected');
"#,
        ])
        .env("AUDIT_SCRIPT", script)
        .env("CHECK_ROOT", &fixture.root)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(result.stdout, b"3 unsafe source requests rejected\n");
    assert_eq!(
        std::fs::read(fixture.root.join("source.txt")).unwrap(),
        b"corrupted source\n"
    );
}

#[test]
fn test_framework_fake_gh_rejects_unknown_commands_and_journals_effects() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[GhStep::read(&["--version"], "gh version fixture\n")]);
    let run = |args: &[&str]| {
        Command::new(fixture.root.join("bin/gh"))
            .args(args)
            .env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
            .stdin(Stdio::null())
            .output()
            .unwrap()
    };
    let success = run(&["--version"]);
    assert!(success.status.success());
    assert_eq!(success.stdout, b"gh version fixture\n");
    let rejected = run(&["pr", "merge", "42"]);
    assert_eq!(rejected.status.code(), Some(86));
    let journal = std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let records: Vec<serde_json::Value> = journal
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["matched"], true);
    assert_eq!(records[1]["matched"], false);
    assert_eq!(records[1]["args"], serde_json::json!(["pr", "merge", "42"]));
}

#[test]
fn test_framework_real_git_builder_preserves_unusual_paths_and_merge_ancestry() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("first\tfile.txt", b"base\n", "root");
    let root = fixture.git(&["rev-parse", "HEAD"]);
    fixture.git(&["switch", "-c", "feature"]);
    fixture.commit_file("nested/日本語.txt", &[0, 1, 2, 255], "binary addition");
    let feature = fixture.git(&["rev-parse", "HEAD"]);
    fixture.git(&["switch", "main"]);
    fixture.commit_file("other.txt", b"main\n", "main addition");
    fixture.git(&["merge", "--no-ff", "feature", "-m", "join histories"]);
    assert_eq!(fixture.git(&["rev-parse", "HEAD^2"]), feature);
    assert_eq!(fixture.git(&["merge-base", "main", "feature"]), feature);
    assert_eq!(fixture.git(&["rev-list", "--count", "HEAD"]), "4\n");
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    assert_ne!(fixture.git(&["rev-parse", "HEAD"]), root);
    assert!(fixture
        .verify_git_effect(&["rev-parse", "HEAD"], "incorrect expected object\n")
        .is_err());
}
